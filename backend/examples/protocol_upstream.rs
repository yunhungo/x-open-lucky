// @created 2026-10-13
// @description 提供本地 HTTP/1.1 与 h2c 流式协议测试上游。
// @author yunhungo
use axum::{
    body::Body,
    extract::Request,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Router,
};
use bytes::Bytes;
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};

async fn serve(request: Request) -> Response {
    let version = format!("{:?}", request.version());
    let path = request.uri().path().to_owned();
    if path == "/bench" {
        return Bytes::from_static(&[b'x'; 1024]).into_response();
    }
    if path == "/delay" {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    if path == "/large" {
        let chunk = Bytes::from(vec![b'x'; 32768]);
        let stream =
            tokio_stream::iter((0..512).map(move |_| Ok::<_, std::io::Error>(chunk.clone())));
        return (
            [(header::CONTENT_TYPE, "application/octet-stream")],
            Body::from_stream(stream),
        )
            .into_response();
    }
    let headers = request
        .headers()
        .iter()
        .map(|(n, v)| (n.to_string(), v.to_str().unwrap_or_default().to_owned()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut body = request.into_body();
    let mut count = 0;
    let mut digest = Sha256::new();
    while let Some(frame) = body.frame().await {
        match frame {
            Ok(frame) => {
                if let Ok(bytes) = frame.into_data() {
                    count += bytes.len();
                    digest.update(bytes);
                }
            }
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    }
    let value = serde_json::json!({"version":version,"headers":headers,"bytes":count,"sha256":hex::encode(digest.finalize())});
    (
        [("x-upstream", "present"), ("server", "test-upstream")],
        axum::Json(value),
    )
        .into_response()
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:59090".into());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    axum::serve(listener, Router::new().fallback(serve)).await?;
    Ok(())
}
