// @created 2026-10-11
// @description 实现管理员初始化、限流登录、会话与密码更新。
// @author yunhungo
use crate::{error::ApiError, AppState};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{ConnectInfo, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use rand::{rngs::OsRng, RngCore};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;

pub fn random_token() -> String {
    let mut bytes = [0; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|v| {
            v.trim()
                .strip_prefix("openlucky_session=")
                .map(str::to_owned)
        })
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}
pub fn check_mutation(headers: &HeaderMap) -> Result<(), ApiError> {
    if headers.get("x-openlucky").and_then(|v| v.to_str().ok()) != Some("1") {
        return Err(ApiError(StatusCode::FORBIDDEN, "请求校验失败".into()));
    }
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        let parsed = url::Url::parse(origin).map_err(|_| ApiError::bad("无效来源"))?;
        let authority = parsed[url::Position::BeforeHost..url::Position::AfterPort].to_owned();
        if headers.get(header::HOST).and_then(|v| v.to_str().ok()) != Some(authority.as_str()) {
            return Err(ApiError(StatusCode::FORBIDDEN, "请求来源不匹配".into()));
        }
    }
    Ok(())
}
pub async fn guard(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: Next,
) -> Result<Response, ApiError> {
    if request.method() != axum::http::Method::GET {
        check_mutation(request.headers())?;
    }
    let token = cookie_token(request.headers()).ok_or_else(ApiError::unauthorized)?;
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE token=? AND expires>?")
            .bind(hash_token(&token))
            .bind(chrono::Utc::now().timestamp())
            .fetch_one(&state.store.pool)
            .await?;
    if count != 1 {
        return Err(ApiError::unauthorized());
    }
    Ok(next.run(request).await)
}
pub async fn initialized(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin")
        .fetch_one(&state.store.pool)
        .await?;
    Ok(Json(json!({"initialized":count > 0})))
}
#[derive(Deserialize)]
pub struct Login {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub setup_token: String,
}
pub fn valid_password(password: &str) -> Result<(), ApiError> {
    if password.len() < 12 || password.len() > 256 {
        return Err(ApiError::bad("密码需为 12–256 个字符"));
    }
    Ok(())
}
pub async fn password_hash(password: String) -> Result<String, ApiError> {
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map(|v| v.to_string())
            .map_err(|_| ApiError::bad("密码处理失败"))
    })
    .await
    .map_err(|_| ApiError::bad("密码处理失败"))?
}
async fn verify(password: String, hashed: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hashed).is_ok_and(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false)
}
async fn session(state: &AppState) -> Result<Response, ApiError> {
    let token = random_token();
    let now = chrono::Utc::now().timestamp();
    sqlx::query("DELETE FROM sessions WHERE expires<=?")
        .bind(now)
        .execute(&state.store.pool)
        .await?;
    sqlx::query("INSERT INTO sessions(token,expires,created) VALUES(?,?,?)")
        .bind(hash_token(&token))
        .bind(now + 43200)
        .bind(now)
        .execute(&state.store.pool)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE token NOT IN (SELECT token FROM sessions ORDER BY created DESC LIMIT 20)").execute(&state.store.pool).await?;
    let secure = if state.config.secure_cookie {
        "; Secure"
    } else {
        ""
    };
    Ok(([(header::SET_COOKIE, format!("openlucky_session={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200{secure}"))], Json(json!({"ok":true}))).into_response())
}
pub async fn setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    crate::error::ApiJson(body): crate::error::ApiJson<Login>,
) -> Result<Response, ApiError> {
    check_mutation(&headers)?;
    valid_password(&body.password)?;
    if body.username.len() < 3
        || body.username.len() > 50
        || !body
            .username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(ApiError::bad(
            "用户名需为 3–50 位字母、数字、下划线或短横线",
        ));
    }
    let expected = state.setup_hash.read().await.clone();
    if expected.is_empty()
        || !bool::from(
            hash_token(&body.setup_token)
                .as_bytes()
                .ct_eq(expected.as_bytes()),
        )
    {
        return Err(ApiError(StatusCode::FORBIDDEN, "初始化密钥无效".into()));
    }
    let hashed = password_hash(body.password).await?;
    let result = sqlx::query("INSERT INTO admin(id,username,password) VALUES(1,?,?)")
        .bind(body.username)
        .bind(hashed)
        .execute(&state.store.pool)
        .await;
    if result.is_err() {
        return Err(ApiError(StatusCode::CONFLICT, "管理员已初始化".into()));
    }
    state.setup_hash.write().await.clear();
    let _ = tokio::fs::remove_file(state.config.data_dir.join("setup-token")).await;
    state.store.event("info", "auth", "管理员已初始化").await;
    session(&state).await
}
pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    crate::error::ApiJson(body): crate::error::ApiJson<Login>,
) -> Result<Response, ApiError> {
    check_mutation(&headers)?;
    let key = peer.ip();
    {
        let mut attempts = state.attempts.lock().await;
        attempts.retain(|_, (time, _)| time.elapsed() < Duration::from_secs(900));
        let entry = attempts.entry(key).or_insert((Instant::now(), 0));
        if entry.1 >= 8 {
            return Err(ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "登录尝试过多，请 15 分钟后重试".into(),
            ));
        }
        entry.1 += 1;
    }
    if body.password.len() > 256 || body.username.len() > 50 {
        return Err(ApiError::unauthorized());
    }
    let admin = sqlx::query("SELECT username,password FROM admin WHERE id=1")
        .fetch_optional(&state.store.pool)
        .await?;
    let hashed = admin
        .as_ref()
        .map(|r| r.get::<String, _>("password"))
        .unwrap_or(state.dummy_hash.clone());
    let valid = verify(body.password, hashed).await;
    if !valid || admin.is_none_or(|r| r.get::<String, _>("username") != body.username) {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "用户名或密码错误".into(),
        ));
    }
    state.attempts.lock().await.remove(&key);
    state.store.event("info", "auth", "管理员登录").await;
    session(&state).await
}
pub async fn me(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let username: String = sqlx::query_scalar("SELECT username FROM admin WHERE id=1")
        .fetch_one(&state.store.pool)
        .await?;
    Ok(Json(json!({"username":username})))
}
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(token) = cookie_token(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token=?")
            .bind(hash_token(&token))
            .execute(&state.store.pool)
            .await?;
    }
    let secure = if state.config.secure_cookie {
        "; Secure"
    } else {
        ""
    };
    Ok((
        [(
            header::SET_COOKIE,
            format!("openlucky_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0{secure}"),
        )],
        Json(json!({"ok":true})),
    )
        .into_response())
}
#[derive(Deserialize)]
pub struct PasswordChange {
    pub current_password: String,
    pub new_password: String,
}
pub async fn change_password(
    State(state): State<AppState>,
    crate::error::ApiJson(body): crate::error::ApiJson<PasswordChange>,
) -> Result<Response, ApiError> {
    valid_password(&body.new_password)?;
    let hashed: String = sqlx::query_scalar("SELECT password FROM admin WHERE id=1")
        .fetch_one(&state.store.pool)
        .await?;
    if !verify(body.current_password, hashed).await {
        return Err(ApiError::bad("当前密码错误"));
    }
    let new_hash = password_hash(body.new_password).await?;
    let mut tx = state.store.pool.begin().await?;
    sqlx::query("UPDATE admin SET password=? WHERE id=1")
        .bind(new_hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    state
        .store
        .event("info", "auth", "管理员更新密码，旧会话已失效")
        .await;
    session(&state).await
}
pub async fn revoke_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let token = cookie_token(&headers).ok_or_else(ApiError::unauthorized)?;
    sqlx::query("DELETE FROM sessions WHERE token<>?")
        .bind(hash_token(&token))
        .execute(&state.store.pool)
        .await?;
    Ok(Json(json!({"ok":true})))
}
