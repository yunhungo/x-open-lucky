// @created 2026-10-11
// @description 启动管理 API、双栈 Web 代理、TLS 和后台任务。
// @author yunhungo
mod auth;
mod backup;
mod certificates;
mod dns;
mod error;
mod gateway_limits;
mod groups;
mod header_rules;
mod http3;
mod jobs;
mod listeners;
mod models;
mod proxy;
mod resources;
mod store;
mod telemetry;
mod template_policy;
mod web_options;
use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::{header, HeaderValue},
    middleware,
    routing::{get, post, put},
    Extension, Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct Config {
    data_dir: PathBuf,
    admin_addr: SocketAddr,
    http_addr: SocketAddr,
    https_addr: SocketAddr,
    http3_addr: SocketAddr,
    public_http3_port: u16,
    public_https_port: u16,
    listeners: Vec<listeners::Listener>,
    secure_cookie: bool,
    build_sha: String,
    lego_bin: String,
    gateway_limits: gateway_limits::GatewayLimits,
}
#[derive(Clone)]
pub struct AppState {
    store: store::Store,
    telemetry: Arc<telemetry::Telemetry>,
    config: Config,
    http: reqwest::Client,
    ipv4: reqwest::Client,
    ipv6: reqwest::Client,
    rules: Arc<RwLock<proxy::Routes>>,
    resolver: Arc<proxy::Resolver>,
    proxy_clients: Arc<RwLock<HashMap<(String, u64), proxy::ProxyClient>>>,
    h3_resolver: Arc<proxy::Resolver>,
    tls_configs: Arc<std::sync::RwLock<proxy::TlsConfigs>>,
    h3_connections: Arc<Semaphore>,
    h3_requests: Arc<Semaphore>,
    jobs: Arc<Mutex<HashSet<String>>>,
    job_limit: Arc<Semaphore>,
    acme_limit: Arc<Semaphore>,
    tasks: TaskTracker,
    shutdown: CancellationToken,
    proxy_id: String,
    mutations: Arc<Mutex<()>>,
    auth_mutations: Arc<Mutex<()>>,
    attempts: Arc<Mutex<HashMap<IpAddr, (Instant, u32)>>>,
    setup_hash: Arc<RwLock<String>>,
    dummy_hash: String,
    started: Instant,
}
fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.into())
}
async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"ok":true,"build_sha":state.config.build_sha,"version":env!("CARGO_PKG_VERSION")}))
}
pub fn management(state: AppState) -> Router {
    let protected = Router::new()
        .route("/auth/me", get(auth::me))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/password", put(auth::change_password))
        .route("/auth/sessions/revoke", post(auth::revoke_sessions))
        .route("/overview", get(resources::overview))
        .route("/logs", get(resources::logs))
        .route("/access-logs", get(telemetry::api::logs))
        .route("/traffic", get(telemetry::api::traffic))
        .route("/group-stats", get(telemetry::api::group_stats))
        .route("/groups", get(groups::list).post(groups::create))
        .route("/groups/order", put(groups::reorder))
        .route("/groups/assign", put(groups::assign))
        .route("/groups/{id}", put(groups::rename).delete(groups::delete))
        .route("/telemetry", get(telemetry::api::health))
        .route("/interfaces", get(resources::interfaces))
        .route("/backup/export", post(backup::export))
        .route("/backup/restore", post(backup::restore))
        .route(
            "/resources/{kind}",
            get(resources::list).post(resources::create),
        )
        .route(
            "/resources/{kind}/{id}",
            put(resources::update).delete(resources::delete),
        )
        .route("/resources/{kind}/{id}/run", post(resources::run))
        .route("/certificates/{id}/import", post(certificates::import))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::guard));
    let api = protected
        .route("/auth/status", get(auth::initialized))
        .route("/auth/login", post(auth::login))
        .route("/auth/setup", post(auth::setup))
        .fallback(|| async {
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(json!({"error":"接口不存在"})),
            )
        });
    let frontend = env("OPENLUCKY_FRONTEND", "frontend/dist");
    Router::new().nest("/api/v1",api).route("/healthz",get(health)).fallback_service(ServeDir::new(&frontend).not_found_service(ServeFile::new(format!("{frontend}/index.html"))))
        .layer(axum::extract::DefaultBodyLimit::max(5*1024*1024))
        .layer(SetResponseHeaderLayer::if_not_present(header::X_CONTENT_TYPE_OPTIONS,HeaderValue::from_static("nosniff")))
        .layer(SetResponseHeaderLayer::if_not_present(header::REFERRER_POLICY,HeaderValue::from_static("same-origin")))
        .layer(SetResponseHeaderLayer::if_not_present(header::X_FRAME_OPTIONS,HeaderValue::from_static("DENY")))
        .layer(SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL,HeaderValue::from_static("no-store")))
        .layer(SetResponseHeaderLayer::if_not_present(header::CONTENT_SECURITY_POLICY,HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'")))
        .layer(TraceLayer::new_for_http()).with_state(state)
}
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openlucky=info,tower_http=warn".into()),
        )
        .init();
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();
    let https_addr = env("OPENLUCKY_HTTPS_ADDR", "[::]:58444").parse()?;
    let public_https_port = env("OPENLUCKY_PUBLIC_HTTPS_PORT", "58444").parse()?;
    let config = Config {
        data_dir: PathBuf::from(env("OPENLUCKY_DATA", "data")),
        admin_addr: env("OPENLUCKY_ADMIN_ADDR", "0.0.0.0:56002").parse()?,
        http_addr: env("OPENLUCKY_HTTP_ADDR", "[::]:58081").parse()?,
        https_addr,
        public_https_port,
        http3_addr: match env("OPENLUCKY_HTTP3_ADDR", "").as_str() {
            "" => https_addr,
            raw => raw.parse()?,
        },
        public_http3_port: match env("OPENLUCKY_PUBLIC_HTTP3_PORT", "").as_str() {
            "" => public_https_port,
            raw => raw.parse()?,
        },
        listeners: listeners::parse(&env("OPENLUCKY_LISTENERS", "[]"))?,
        secure_cookie: env("OPENLUCKY_SECURE_COOKIE", "false") == "true",
        build_sha: env(
            "OPENLUCKY_BUILD_SHA",
            option_env!("BUILD_SHA").unwrap_or("dev"),
        ),
        lego_bin: env("OPENLUCKY_LEGO_BIN", "lego"),
        gateway_limits: gateway_limits::GatewayLimits::from_env()?,
    };
    anyhow::ensure!(
        config.http3_addr.port() != 0 && config.public_http3_port != 0,
        "HTTP/3 端口不能为 0"
    );
    let store = store::Store::open(&config.data_dir).await?;
    let initialized: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin")
        .fetch_one(&store.pool)
        .await?;
    let path = config.data_dir.join("setup-token");
    let setup = if initialized == 0 {
        if path.exists() {
            tokio::fs::read_to_string(&path).await?
        } else {
            let token = auth::random_token();
            store::private_write(&path, token.as_bytes()).await?;
            tracing::info!("Setup token is stored in data/setup-token");
            token
        }
    } else {
        String::new()
    };
    let base = || {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
    };
    let telemetry =
        telemetry::Telemetry::open(&config.data_dir, telemetry::Policy::from_env()?).await?;
    let state = AppState {
        store,
        telemetry,
        config: config.clone(),
        http: base().build()?,
        ipv4: base().local_address("0.0.0.0".parse::<IpAddr>()?).build()?,
        ipv6: base().local_address("::".parse::<IpAddr>()?).build()?,
        rules: Arc::default(),
        resolver: Arc::default(),
        proxy_clients: Arc::default(),
        h3_resolver: Arc::default(),
        tls_configs: Arc::default(),
        h3_connections: Arc::new(Semaphore::new(config.gateway_limits.connections)),
        h3_requests: Arc::new(Semaphore::new(config.gateway_limits.requests)),
        jobs: Arc::default(),
        job_limit: Arc::new(Semaphore::new(4)),
        acme_limit: Arc::new(Semaphore::new(1)),
        tasks: TaskTracker::new(),
        shutdown: CancellationToken::new(),
        proxy_id: auth::random_token(),
        mutations: Arc::default(),
        auth_mutations: Arc::default(),
        attempts: Arc::default(),
        setup_hash: Arc::new(RwLock::new(if setup.is_empty() {
            String::new()
        } else {
            auth::hash_token(&setup)
        })),
        dummy_hash: auth::password_hash(auth::random_token())
            .await
            .map_err(|e| anyhow::anyhow!(e.1))?,
        started: Instant::now(),
    };
    // 重启后运行中的任务重新进入调度，不让上次进程状态永久挂起。
    for kind in ["ddns", "certificates"] {
        for item in state.store.list(kind).await? {
            if item["status"]["state"] == "running" {
                let mut status = item["status"].clone();
                status["state"] = json!("idle");
                state
                    .store
                    .status(
                        kind,
                        item["id"].as_str().context("Invalid resource ID")?,
                        status,
                    )
                    .await?;
            }
        }
    }
    proxy::refresh(&state).await?;
    let telemetry_task = tokio::spawn(state.telemetry.clone().run(state.shutdown.clone()));
    tokio::spawn(jobs::scheduler(state.clone()));
    let plain = Router::new()
        .fallback(proxy::forward)
        .layer(Extension(listeners::Ingress {
            listener: "default".into(),
            tls: false,
            tls13: false,
            https_port: config.public_https_port,
            http3_port: config.public_http3_port,
        }))
        .with_state(state.clone());
    let admin = tokio::net::TcpListener::bind(config.admin_addr).await?;
    let http = tokio::net::TcpListener::bind(config.http_addr).await?;
    state.store.event("info", "system", "服务已启动").await;
    tracing::info!(admin=%config.admin_addr,http=%config.http_addr,https=%config.https_addr,"OpenLucky ready");
    let shutdown = state.shutdown.clone();
    let signal_token = shutdown.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            if let Ok(mut term) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            {
                tokio::select! {_=tokio::signal::ctrl_c()=>(),_=term.recv()=>()}
            } else {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        signal_token.cancel();
    });
    let admin_shutdown = shutdown.clone();
    let http_shutdown = shutdown.clone();
    tokio::try_join!(
        async {
            axum::serve(
                admin,
                management(state.clone()).into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move { admin_shutdown.cancelled().await })
            .await
            .map_err(anyhow::Error::from)
        },
        async {
            axum::serve(
                http,
                plain.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move { http_shutdown.cancelled().await })
            .await
            .map_err(anyhow::Error::from)
        },
        listeners::serve(state.clone())
    )?;
    state.tasks.close();
    state.tasks.wait().await;
    let _ = telemetry_task.await;
    state.telemetry.close().await;
    state.store.pool.close().await;
    Ok(())
}
