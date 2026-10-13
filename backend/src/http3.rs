// @created 2026-10-13
// @description 接收有并发与流控上限的 QUIC 请求并复用流式反向代理。
// @author yunhungo
use crate::{
    listeners::{Ingress, Listener},
    proxy, AppState,
};
use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    Extension,
};
use bytes::{Buf, Bytes};
use http_body_util::{BodyExt, StreamBody};
use std::{io, sync::Arc, time::Duration};

pub fn endpoint(state: &AppState, listener: &Listener) -> anyhow::Result<quinn::Endpoint> {
    let resolver = proxy::ScopedResolver {
        inner: state.h3_resolver.clone(),
        listener: listener.id.clone(),
    };
    let mut tls = rustls::ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(resolver));
    tls.alpn_protocols = vec![b"h3".to_vec()];
    // Keep 0-RTT disabled: proxy requests may mutate upstream state.
    let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls)?;
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    let mut transport = quinn::TransportConfig::default();
    let limits = &state.config.gateway_limits;
    transport
        .max_concurrent_bidi_streams(limits.streams.into())
        .max_concurrent_uni_streams(8u32.into())
        .stream_receive_window(limits.stream_window.into())
        .receive_window(limits.receive_window.into())
        .send_window(limits.send_window)
        .max_idle_timeout(Some(Duration::from_secs(30).try_into()?));
    config.transport_config(Arc::new(transport));
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(listener.quic_addr()),
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    if listener.quic_addr().is_ipv6() {
        socket.set_only_v6(true)?;
    }
    socket.set_nonblocking(true)?;
    socket.bind(&listener.quic_addr().into())?;
    Ok(quinn::Endpoint::new(
        quinn::EndpointConfig::default(),
        Some(config),
        socket.into(),
        Arc::new(quinn::TokioRuntime),
    )?)
}

pub async fn serve(
    state: AppState,
    listener: Listener,
    endpoint: quinn::Endpoint,
) -> io::Result<()> {
    let connections = state.h3_connections.clone();
    let requests = state.h3_requests.clone();
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = state.shutdown.cancelled() => break,
            _ = tasks.join_next(), if !tasks.is_empty() => {},
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break; };
                let Ok(permit) = connections.clone().try_acquire_owned() else { incoming.refuse(); continue; };
                // Address validation avoids allocating a full handshake for spoofed UDP packets.
                if !incoming.remote_address_validated() { let _ = incoming.retry(); continue; }
                let state = state.clone();
                let listener = listener.clone();
                let requests = requests.clone();
                tasks.spawn(async move {
                    let _permit = permit;
                    let Ok(Ok(connection)) = tokio::time::timeout(Duration::from_secs(10), incoming).await else { return; };
                    let peer = connection.remote_address();
                    let Ok(mut h3) = h3::server::builder().max_field_section_size(32 * 1024)
                        .build(h3_quinn::Connection::new(connection)).await else { return; };
                    let mut streams = tokio::task::JoinSet::new();
                    loop {
                        tokio::select! {
                            _ = state.shutdown.cancelled() => break,
                            _ = streams.join_next(), if !streams.is_empty() => {},
                            request = h3.accept() => {
                                let Ok(Some(resolver)) = request else { break; };
                                let state = state.clone();
                                let listener = listener.clone();
                                let permit = requests.clone().try_acquire_owned();
                                streams.spawn(async move {
                                    let Ok((mut request, stream)) = resolver.resolve_request().await else { return; };
                                    let (mut send, mut recv) = stream.split();
                                    let Ok(_permit) = permit else {
                                        let response = axum::http::Response::builder().status(503).body(()).unwrap();
                                        let _ = send.send_response(response).await;
                                        let _ = send.finish().await;
                                        return;
                                    };
                                    *request.version_mut() = axum::http::Version::HTTP_3;
                                    let (tx, rx) = tokio::sync::mpsc::channel::<Result<hyper::body::Frame<Bytes>, io::Error>>(2);
                                    let read_body = async move {
                                        loop {
                                            match recv.recv_data().await {
                                                Ok(Some(mut data)) => {
                                                    let data = data.copy_to_bytes(data.remaining());
                                                    if tx.send(Ok(hyper::body::Frame::data(data))).await.is_err() { return; }
                                                }
                                                Ok(None) => break,
                                                Err(_) => { let _ = tx.send(Err(io::Error::other("HTTP/3 request interrupted"))).await; return; }
                                            }
                                        }
                                        match recv.recv_trailers().await {
                                            Ok(Some(trailers)) => { let _ = tx.send(Ok(hyper::body::Frame::trailers(trailers))).await; }
                                            Err(_) => { let _ = tx.send(Err(io::Error::other("HTTP/3 trailers interrupted"))).await; }
                                            _ => {}
                                        }
                                    };
                                    let respond = async {
                                        let body = Body::new(StreamBody::new(tokio_stream::wrappers::ReceiverStream::new(rx)));
                                        let response = proxy::forward(State(state), ConnectInfo(peer), Extension(Ingress {
                                            http3_port: listener.quic_port(), listener: listener.id, tls: true, tls13: true, https_port: listener.https_port,
                                        }), request.map(|_| body)).await;
                                        let (mut parts, mut body) = response.into_parts();
                                        parts.version = axum::http::Version::HTTP_3;
                                        send.send_response(axum::http::Response::from_parts(parts, ())).await?;
                                        while let Some(frame) = body.frame().await {
                                            let frame = frame.map_err(|_| anyhow::anyhow!("Upstream stream interrupted"))?;
                                            match frame.into_data() {
                                                Ok(data) => send.send_data(data).await?,
                                                Err(frame) => if let Ok(trailers) = frame.into_trailers() { send.send_trailers(trailers).await?; return Ok::<_, anyhow::Error>(()); },
                                            }
                                        }
                                        send.finish().await?;
                                        Ok(())
                                    };
                                    tokio::pin!(read_body, respond);
                                    tokio::select! {
                                        _ = &mut respond => {},
                                        _ = &mut read_body => { let _ = respond.await; },
                                    }
                                });
                            }
                        }
                    }
                });
            }
        }
    }
    endpoint.close(0u32.into(), b"shutdown");
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    let _ = tokio::time::timeout(Duration::from_secs(5), endpoint.wait_idle()).await;
    Ok(())
}
