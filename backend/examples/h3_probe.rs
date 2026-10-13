// @created 2026-10-13
// @description 用真实 QUIC 客户端验证流式请求、并发性能和证书校验。
// @author yunhungo
use anyhow::{Context, Result};
use bytes::{Buf, Bytes};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Options {
    address: std::net::SocketAddr,
    server_name: String,
    #[serde(default)]
    ca: String,
    #[serde(default = "path")]
    path: String,
    #[serde(default)]
    upload_bytes: usize,
    #[serde(default = "one")]
    requests: usize,
    #[serde(default = "one")]
    concurrency: usize,
}
fn path() -> String {
    "/".into()
}
fn one() -> usize {
    1
}
#[tokio::main]
async fn main() -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let options: Options = serde_json::from_str(&input)?;
    tokio::time::timeout(Duration::from_secs(60), run(options)).await??;
    Ok(())
}
async fn run(options: Options) -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();
    anyhow::ensure!(
        (1..=32).contains(&options.concurrency) && options.requests > 0,
        "Invalid workload"
    );
    let mut roots =
        rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if !options.ca.is_empty() {
        let pem = std::fs::read(&options.ca)?;
        for cert in rustls_pemfile::certs(&mut pem.as_slice()) {
            roots.add(cert?)?;
        }
    }
    let mut tls = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls.alpn_protocols = vec![b"h3".to_vec()];
    let config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls)?,
    ));
    let mut endpoint = quinn::Endpoint::client(
        if options.address.is_ipv6() {
            "[::]:0"
        } else {
            "0.0.0.0:0"
        }
        .parse()?,
    )?;
    endpoint.set_default_client_config(config);
    let connection = endpoint
        .connect(options.address, &options.server_name)?
        .await?;
    let (mut driver, sender) =
        h3::client::new(h3_quinn::Connection::new(connection.clone())).await?;
    let drive = tokio::spawn(async move { std::future::poll_fn(|cx| driver.poll_close(cx)).await });
    let start = Instant::now();
    let mut tasks = tokio::task::JoinSet::new();
    let mut latencies = Vec::with_capacity(options.requests);
    let mut failures = 0;
    let mut sample = None;
    for index in 0..options.requests {
        let mut sender = sender.clone();
        let uri = format!(
            "https://{}:{}{}",
            options.server_name,
            options.address.port(),
            options.path
        );
        let upload = options.upload_bytes;
        tasks.spawn(async move {
            let start = Instant::now();
            let mut request = axum::http::Request::builder().uri(uri);
            if upload > 0 { request = request.method("POST").header("content-length", upload); }
            let mut stream = sender.send_request(request.body(())?).await.context("send headers")?;
            let chunk = Bytes::from(vec![b'x'; 32 * 1024]);
            let mut remaining = upload;
            while remaining > 0 {
                let size = remaining.min(chunk.len());
                stream.send_data(chunk.slice(..size)).await?;
                remaining -= size;
            }
            // A complete early response can stop the request side before FIN is flushed.
            // Keep validating the response; a reset of its receive side still fails.
            if let Err(error) = stream.finish().await {
                match error {
                    h3::error::StreamError::RemoteTerminate { code, .. }
                        if code.value() == 0 || code == h3::error::Code::H3_NO_ERROR => {}
                    other => return Err(other).context("finish request"),
                }
            }
            let response = stream.recv_response().await.context("receive headers")?;
            let mut bytes = 0;
            let mut prefix = Vec::new();
            let mut digest = Sha256::new();
            while let Some(mut chunk) = stream.recv_data().await.context("receive body")? {
                let chunk = chunk.copy_to_bytes(chunk.remaining());
                bytes += chunk.len();
                digest.update(&chunk);
                if prefix.len() < 8192 { prefix.extend_from_slice(&chunk[..chunk.len().min(8192 - prefix.len())]); }
            }
            let _ = stream.recv_trailers().await.context("receive trailers")?;
            let headers = response.headers().iter().map(|(name, value)| (name.to_string(), value.to_str().unwrap_or_default().to_owned())).collect::<std::collections::BTreeMap<_, _>>();
            let ms = start.elapsed().as_secs_f64()*1000.0;
            let sample = (index == 0).then(|| serde_json::json!({"status":response.status().as_u16(),"headers":headers,"bytes":bytes,"body":String::from_utf8_lossy(&prefix),"sha256":hex::encode(digest.finalize()),"duration_ms":ms}));
            Ok::<_, anyhow::Error>((ms, !response.status().is_success(), sample))
        });
        if tasks.len() >= options.concurrency || index + 1 == options.requests {
            while let Some(result) = tasks.join_next().await {
                let (ms, failed, response) =
                    result?.with_context(|| format!("request batch ending at {}", index + 1))?;
                latencies.push(ms);
                failures += usize::from(failed);
                if response.is_some() {
                    sample = response;
                }
            }
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    latencies.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({"protocol":"HTTP/3","requests":latencies.len(),"failures":failures,"seconds":elapsed,"rps":latencies.len() as f64/elapsed,"p95_ms":latencies[(latencies.len()*95/100).min(latencies.len()-1)],"response":sample.context("No response")?})
    );
    connection.close(0u32.into(), b"done");
    drive.abort();
    endpoint.wait_idle().await;
    Ok(())
}
