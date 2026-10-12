// @created 2026-10-11
// @description 运行流式反向代理、WebSocket 隧道与动态 SNI 证书解析。
// @author yunhungo
use crate::telemetry::{Access, Telemetry};
use crate::{
    models::{Certificate, WebRule},
    AppState,
};
use anyhow::Result;
use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension,
};
use http_body_util::BodyExt;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::{TokioExecutor, TokioIo},
};
use rustls::{
    server::{ClientHello, ResolvesServerCert},
    sign::CertifiedKey,
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};

pub type ProxyClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, Body>;
#[derive(Debug, Default)]
pub struct Resolver(pub RwLock<HashMap<(String, String), ActiveCertificate>>);
#[derive(Debug, Clone)]
pub struct ActiveCertificate {
    pub key: Arc<CertifiedKey>,
    pub expires_at: i64,
}
impl ActiveCertificate {
    pub fn valid(&self) -> bool {
        self.expires_at > chrono::Utc::now().timestamp()
    }
}
#[derive(Debug)]
pub struct ScopedResolver {
    pub inner: Arc<Resolver>,
    pub listener: String,
}
impl ResolvesServerCert for ScopedResolver {
    fn resolve(&self, hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        self.inner
            .0
            .read()
            .ok()?
            .get(&(self.listener.clone(), hello.server_name()?.to_lowercase()))
            .filter(|cert| cert.valid())
            .map(|cert| cert.key.clone())
    }
}
pub fn client() -> ProxyClient {
    let mut http = HttpConnector::new();
    http.enforce_http(false);
    http.set_connect_timeout(Some(Duration::from_secs(10)));
    let connector = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .wrap_connector(http);
    Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(60))
        .build(connector)
}
pub async fn refresh(state: &AppState) -> Result<()> {
    let values = state.store.list("web").await?;
    let mut rules = Vec::new();
    let mut certificates = HashMap::new();
    for value in values {
        let rule: WebRule = serde_json::from_value(value)?;
        if !rule.enabled {
            continue;
        }
        if !rule.certificate_id.is_empty() {
            if let Ok(cert) = state
                .store
                .get::<Certificate>("certificates", &rule.certificate_id)
                .await
            {
                if let Ok(key) = crate::certificates::load(&state.config.data_dir, &cert).await {
                    certificates.insert((rule.listener.clone(), rule.domain.clone()), key);
                }
            }
        }
        rules.push(rule);
    }
    *state.rules.write().await = rules;
    *state
        .resolver
        .0
        .write()
        .map_err(|_| anyhow::anyhow!("Certificate lock failed"))? = certificates;
    Ok(())
}
fn strip_hop(headers: &mut HeaderMap, websocket: bool) {
    let tokens = headers
        .get(header::CONNECTION)
        .and_then(|v| v.to_str().ok())
        .map(|s| {
            s.split(',')
                .map(|s| s.trim().to_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for token in tokens {
        if !websocket || !token.eq_ignore_ascii_case("upgrade") {
            headers.remove(token.as_str());
        }
    }
    for name in [
        "proxy-connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
    ] {
        headers.remove(name);
    }
    if !websocket {
        headers.remove(header::CONNECTION);
        headers.remove(header::UPGRADE);
    }
}
pub async fn forward(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(ingress): Extension<crate::listeners::Ingress>,
    request: Request,
) -> Response {
    let started = std::time::Instant::now();
    let mut entry = Access {
        time: chrono::Utc::now().timestamp(),
        request_id: uuid::Uuid::new_v4().to_string(),
        service_id: String::new(),
        host: String::new(),
        listener: ingress.listener.clone(),
        client_ip: peer.ip().to_string(),
        method: Telemetry::bounded(request.method().as_str(), 16),
        path: Telemetry::bounded(request.uri().path(), 512),
        status: 0,
        duration_ms: 0.0,
        error: String::new(),
        phase: "request".into(),
        tls: ingress.tls,
    };
    let mut response = forward_inner(&state, peer, ingress, request, &mut entry).await;
    entry.duration_ms = started.elapsed().as_secs_f64() * 1000.0;
    entry.status = response.status().as_u16();
    if entry.status >= 400 && entry.error.is_empty() {
        entry.error = match entry.status {
            400 => "无效 Host",
            404 => "未匹配域名",
            503 => "证书尚未就绪",
            508 => "代理循环已阻止",
            _ => "代理配置无效",
        }
        .into();
    }
    // 由本代理生成关联 ID，忽略外部注入的同名响应头。
    if let Ok(id) = HeaderValue::from_str(&entry.request_id) {
        response.headers_mut().insert("x-openlucky-request-id", id);
    }
    state.telemetry.record(entry.clone());
    let telemetry = state.telemetry.clone();
    response.map(|body| {
        Body::new(body.map_err(move |error| {
            let mut failure = entry.clone();
            failure.time = chrono::Utc::now().timestamp();
            failure.phase = "stream".into();
            failure.error = "响应流中断".into();
            telemetry.record(failure);
            error
        }))
    })
}
async fn forward_inner(
    state: &AppState,
    peer: SocketAddr,
    ingress: crate::listeners::Ingress,
    mut request: Request,
    entry: &mut Access,
) -> Response {
    let tls = ingress.tls;
    if request
        .headers()
        .get("x-openlucky-via")
        .and_then(|h| h.to_str().ok())
        == Some(state.proxy_id.as_str())
    {
        return (StatusCode::LOOP_DETECTED, "代理循环已阻止").into_response();
    }
    let authority = request.uri().authority().cloned().or_else(|| {
        request
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<axum::http::uri::Authority>().ok())
    });
    let Some(authority) = authority else {
        return (StatusCode::BAD_REQUEST, "无效 Host").into_response();
    };
    let domain = authority.host().to_lowercase();
    entry.host = Telemetry::bounded(&domain, 253);
    let rule = state
        .rules
        .read()
        .await
        .iter()
        .find(|r| r.domain == domain && r.listener == ingress.listener)
        .cloned();
    let Some(rule) = rule else {
        return (StatusCode::NOT_FOUND, "未配置此域名").into_response();
    };
    entry.service_id = rule.id.clone();
    let path = request
        .uri()
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/")
        .to_owned();
    if !tls && rule.https_redirect {
        let ready = state.resolver.0.read().is_ok_and(|r| {
            r.get(&(ingress.listener.clone(), domain.clone()))
                .is_some_and(ActiveCertificate::valid)
        });
        if !ready {
            return (StatusCode::SERVICE_UNAVAILABLE, "证书尚未就绪").into_response();
        }
        let port = ingress.https_port;
        let host = if port == 443 {
            domain.clone()
        } else {
            format!("{domain}:{port}")
        };
        return (
            StatusCode::PERMANENT_REDIRECT,
            [(header::LOCATION, format!("https://{host}{path}"))],
        )
            .into_response();
    }
    let target = match url::Url::parse(&rule.target) {
        Ok(t) => t,
        Err(_) => return (StatusCode::BAD_GATEWAY, "代理配置无效").into_response(),
    };
    let upstream = format!("{}{}", rule.target.trim_end_matches('/'), path);
    let uri = match upstream.parse::<axum::http::Uri>() {
        Ok(uri) => uri,
        Err(_) => return (StatusCode::BAD_GATEWAY, "代理地址无效").into_response(),
    };
    // Headscale uses tailscale-control-protocol; HTTP/1 upgrades are not limited to WebSocket.
    let upgrade = request
        .headers()
        .get(header::UPGRADE)
        .is_some_and(|v| !v.is_empty())
        && request
            .headers()
            .get(header::CONNECTION)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
            });
    let client_upgrade = if upgrade {
        Some(hyper::upgrade::on(&mut request))
    } else {
        None
    };
    strip_hop(request.headers_mut(), upgrade);
    if let Ok(value) = HeaderValue::from_str(&state.proxy_id) {
        request.headers_mut().insert("x-openlucky-via", value);
    }
    for name in [
        "forwarded",
        "x-forwarded-for",
        "x-forwarded-host",
        "x-forwarded-proto",
        "x-real-ip",
    ] {
        request.headers_mut().remove(name);
    }
    if let Ok(value) = HeaderValue::from_str(&peer.ip().to_string()) {
        request
            .headers_mut()
            .insert("x-forwarded-for", value.clone());
        request.headers_mut().insert("x-real-ip", value);
    }
    if let Ok(value) = HeaderValue::from_str(authority.as_str()) {
        request.headers_mut().insert("x-forwarded-host", value);
    }
    request.headers_mut().insert(
        "x-forwarded-proto",
        HeaderValue::from_static(if tls { "https" } else { "http" }),
    );
    let host = if rule.preserve_host {
        authority.as_str()
    } else {
        &target[url::Position::BeforeHost..url::Position::AfterPort]
    };
    let Ok(value) = HeaderValue::from_str(host) else {
        return (StatusCode::BAD_GATEWAY, "目标 Host 无效").into_response();
    };
    request.headers_mut().insert(header::HOST, value);
    *request.uri_mut() = uri;
    // 下游 HTTP/2 不代表上游也支持 HTTP/2；由连接器按上游协议协商。
    *request.version_mut() = axum::http::Version::HTTP_11;
    let result =
        tokio::time::timeout(Duration::from_secs(60), state.proxy_client.request(request)).await;
    let mut response = match result {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => {
            entry.error = "上游连接失败（连接、TLS 或协议）".into();
            return (StatusCode::BAD_GATEWAY, "上游连接失败").into_response();
        }
        Err(_) => {
            entry.error = "上游响应超时".into();
            return (StatusCode::GATEWAY_TIMEOUT, "上游响应超时").into_response();
        }
    };
    if response.status().is_client_error() || response.status().is_server_error() {
        entry.error = format!("上游返回 HTTP {}", response.status().as_u16());
    }
    if response.status() == StatusCode::SWITCHING_PROTOCOLS {
        if let Some(downstream) = client_upgrade {
            let upstream = hyper::upgrade::on(&mut response);
            let telemetry = state.telemetry.clone();
            let mut failure = entry.clone();
            failure.status = 101;
            tokio::spawn(async move {
                let failed = match tokio::try_join!(downstream, upstream) {
                    Ok((down, up)) => tokio::io::copy_bidirectional(
                        &mut TokioIo::new(down),
                        &mut TokioIo::new(up),
                    )
                    .await
                    .is_err(),
                    Err(_) => true,
                };
                if failed {
                    failure.time = chrono::Utc::now().timestamp();
                    failure.phase = "tunnel".into();
                    failure.error = "升级隧道中断".into();
                    telemetry.record(failure);
                }
            });
        }
    }
    let upgraded = upgrade && response.status() == StatusCode::SWITCHING_PROTOCOLS;
    strip_hop(response.headers_mut(), upgraded);
    response.map(Body::new)
}
