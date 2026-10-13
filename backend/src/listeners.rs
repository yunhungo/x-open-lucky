// @created 2026-10-12
// @description 隔离代理入口，并用 Tokio/rustls 接收同端口 HTTP 与 HTTPS。
// @author yunhungo
use crate::{proxy, AppState};
use axum::{middleware::AddExtension, Extension, Router};
use axum_server::accept::Accept;
use serde::{Deserialize, Serialize};
use std::{future::Future, io, net::SocketAddr, pin::Pin, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tower::Layer;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Listener {
    pub id: String,
    pub addr: SocketAddr,
    pub https_port: u16,
    #[serde(default)]
    pub mixed: bool,
    #[serde(default)]
    pub http3_addr: Option<SocketAddr>,
    #[serde(default)]
    pub http3_port: Option<u16>,
}
impl Listener {
    pub fn quic_addr(&self) -> SocketAddr {
        self.http3_addr.unwrap_or(self.addr)
    }
    pub fn quic_port(&self) -> u16 {
        self.http3_port.unwrap_or(self.https_port)
    }
}
#[derive(Clone)]
pub struct Ingress {
    pub listener: String,
    pub tls: bool,
    pub tls13: bool,
    pub http3_port: u16,
    pub https_port: u16,
}
pub fn parse(raw: &str) -> anyhow::Result<Vec<Listener>> {
    let listeners: Vec<Listener> = serde_json::from_str(raw)?;
    anyhow::ensure!(listeners.len() <= 16, "监听入口最多 16 个");
    let mut ids = std::collections::HashSet::from(["default"]);
    for listener in &listeners {
        anyhow::ensure!(
            !listener.id.is_empty()
                && listener.id.len() <= 64
                && listener
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                && ids.insert(&listener.id)
                && listener.addr.port() != 0
                && listener.https_port != 0
                && listener.quic_addr().port() != 0
                && listener.quic_port() != 0,
            "监听入口名称或端口无效，名称必须唯一且不能使用 default"
        );
    }
    Ok(listeners)
}
pub trait ProxyIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> ProxyIo for T {}
#[derive(Clone)]
struct Acceptor {
    state: AppState,
    listener: Listener,
}
impl<S: Send + 'static> Accept<TcpStream, S> for Acceptor {
    type Stream = Box<dyn ProxyIo>;
    type Service = AddExtension<S, Ingress>;
    type Future = Pin<Box<dyn Future<Output = io::Result<(Self::Stream, Self::Service)>> + Send>>;
    fn accept(&self, stream: TcpStream, service: S) -> Self::Future {
        let config = self.clone();
        Box::pin(async move {
            stream.set_nodelay(true)?;
            let tls = if config.listener.mixed {
                let mut first = [0];
                let size = tokio::time::timeout(Duration::from_secs(10), stream.peek(&mut first))
                    .await??;
                if size == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                // TLS ClientHello begins with the handshake record type. No bytes are consumed.
                first[0] == 22
            } else {
                true
            };
            let mut tls13 = false;
            let stream: Box<dyn ProxyIo> = if tls {
                let handshake = async {
                    let hello = tokio_rustls::LazyConfigAcceptor::new(
                        rustls::server::Acceptor::default(),
                        stream,
                    )
                    .await?;
                    let domain = hello
                        .client_hello()
                        .server_name()
                        .unwrap_or_default()
                        .to_lowercase();
                    let tls_config = config
                        .state
                        .tls_configs
                        .read()
                        .ok()
                        .and_then(|configs| {
                            configs.get(&(config.listener.id.clone(), domain)).cloned()
                        })
                        .ok_or_else(|| {
                            io::Error::new(io::ErrorKind::PermissionDenied, "Unknown TLS service")
                        })?;
                    hello.into_stream(tls_config).await
                };
                let secure = tokio::time::timeout(Duration::from_secs(10), handshake).await??;
                tls13 =
                    secure.get_ref().1.protocol_version() == Some(rustls::ProtocolVersion::TLSv1_3);
                Box::new(secure)
            } else {
                Box::new(stream)
            };
            let service = Extension(Ingress {
                http3_port: config.listener.quic_port(),
                listener: config.listener.id,
                tls,
                tls13,
                https_port: config.listener.https_port,
            })
            .layer(service);
            Ok((stream, service))
        })
    }
}
pub async fn serve(state: AppState) -> anyhow::Result<()> {
    let mut configs = vec![Listener {
        id: "default".into(),
        addr: state.config.https_addr,
        https_port: state.config.public_https_port,
        mixed: false,
        http3_addr: Some(state.config.http3_addr),
        http3_port: Some(state.config.public_http3_port),
    }];
    configs.extend(state.config.listeners.clone());
    // Bind all addresses before accepting requests, so configuration failure is immediate.
    let mut bound = Vec::new();
    for config in configs {
        let socket = if config.addr.is_ipv6() {
            let socket = tokio::net::TcpSocket::new_v6()?;
            socket.set_reuseaddr(true)?;
            socket2::SockRef::from(&socket).set_only_v6(true)?;
            socket.bind(config.addr)?;
            socket.listen(1024)?.into_std()?
        } else {
            let socket = std::net::TcpListener::bind(config.addr)?;
            socket.set_nonblocking(true)?;
            socket
        };
        bound.push((config, socket));
    }
    let mut servers = tokio::task::JoinSet::new();
    let mut handles = Vec::new();
    for (config, socket) in bound {
        let endpoint = crate::http3::endpoint(&state, &config)?;
        let h3_state = state.clone();
        let h3_config = config.clone();
        servers.spawn(async move { crate::http3::serve(h3_state, h3_config, endpoint).await });
        let acceptor = Acceptor {
            state: state.clone(),
            listener: config,
        };
        let router = Router::new()
            .fallback(proxy::forward)
            .with_state(state.clone());
        let handle = axum_server::Handle::new();
        handles.push(handle.clone());
        servers.spawn(async move {
            axum_server::from_tcp(socket)
                .acceptor(acceptor)
                .handle(handle)
                .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                .await
        });
    }
    tokio::select! {
        result = servers.join_next() => {
            if let Some(result) = result { result??; }
            anyhow::bail!("代理监听入口意外停止");
        }
        _ = state.shutdown.cancelled() => {}
    }
    for handle in handles {
        handle.graceful_shutdown(Some(Duration::from_secs(30)));
    }
    while let Some(result) = servers.join_next().await {
        result??;
    }
    Ok(())
}
