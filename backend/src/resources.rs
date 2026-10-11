// @created 2026-10-11
// @description 提供资源 CRUD、依赖保护和后台任务入口。
// @author yunhungo
use crate::{
    error::ApiError,
    models::{self, Certificate, Credential},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

fn valid_kind(kind: &str) -> Result<(), ApiError> {
    if !["credentials", "ddns", "web", "certificates"].contains(&kind) {
        return Err(ApiError(StatusCode::NOT_FOUND, "资源不存在".into()));
    }
    Ok(())
}
pub async fn list(
    State(state): State<AppState>,
    Path(kind): Path<String>,
) -> Result<Json<Value>, ApiError> {
    valid_kind(&kind)?;
    Ok(Json(json!(checked_list(&state, &kind).await?)))
}
async fn checked_list(state: &AppState, kind: &str) -> anyhow::Result<Vec<Value>> {
    let mut items = state.store.list(kind).await?;
    if kind == "web" {
        let certificates = state
            .resolver
            .0
            .read()
            .map_err(|_| anyhow::anyhow!("Certificate state unavailable"))?;
        for item in &mut items {
            if item["enabled"] == true
                && item["certificate_id"]
                    .as_str()
                    .is_some_and(|id| !id.is_empty())
                && !certificates
                    .get(&(
                        item["listener"].as_str().unwrap_or("default").to_owned(),
                        item["domain"].as_str().unwrap_or_default().to_owned(),
                    ))
                    .is_some_and(crate::proxy::ActiveCertificate::valid)
            {
                item["status"] = json!({"state":"error","message":"绑定证书未就绪或已过期"});
            }
        }
    }
    Ok(items)
}
pub async fn create(
    State(state): State<AppState>,
    Path(kind): Path<String>,
    crate::error::ApiJson(value): crate::error::ApiJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let id = uuid::Uuid::new_v4().to_string();
    save(&state, &kind, &id, value, false).await?;
    Ok(Json(json!({"id":id})))
}
pub async fn update(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    crate::error::ApiJson(value): crate::error::ApiJson<Value>,
) -> Result<Json<Value>, ApiError> {
    save(&state, &kind, &id, value, true).await?;
    Ok(Json(json!({"id":id})))
}
async fn save(
    state: &AppState,
    kind: &str,
    id: &str,
    value: Value,
    update: bool,
) -> Result<(), ApiError> {
    valid_kind(kind)?;
    let _lock = state.mutations.lock().await;
    if state.jobs.lock().await.contains(&format!("{kind}:{id}")) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "任务正在执行，请稍后编辑".into(),
        ));
    }
    let requested_revision = value["revision"].as_i64();
    let mut value = models::validate(kind, value)?;
    if update {
        let current = state.store.get::<Value>(kind, id).await?;
        if requested_revision != current["revision"].as_i64() {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "配置已被其他操作更新，请关闭表单后重新编辑".into(),
            ));
        }
    }
    value["id"] = json!(id);
    if kind == "credentials" {
        let previous = if update {
            Some(state.store.get::<Credential>(kind, id).await?)
        } else {
            None
        };
        let provider = value["provider"].as_str().unwrap_or_default().to_owned();
        if previous.as_ref().is_some_and(|p| p.provider != provider) {
            return Err(ApiError::bad("凭据服务商不能修改，请新建凭据"));
        }
        for &field in crate::dns::provider::Provider::parse(&provider)?.fields() {
            if value["secrets"][field].as_str().is_none_or(str::is_empty) {
                if let Some(secret) = previous.as_ref().and_then(|p| p.secrets.get(field)) {
                    value["secrets"][field] = json!(secret);
                } else {
                    return Err(ApiError::bad(format!("缺少凭据字段：{field}")));
                }
            }
        }
        let c: Credential =
            serde_json::from_value(value.clone()).map_err(|_| ApiError::bad("凭据格式错误"))?;
        if c.secrets
            .values()
            .any(|v| v.len() > 2048 || v.contains('\n'))
        {
            return Err(ApiError::bad("凭据格式无效"));
        }
    } else if kind == "ddns" || (kind == "certificates" && value["imported"] != true) {
        let cid = value["credential_id"].as_str().unwrap_or_default();
        let credential = state.store.get::<Credential>("credentials", cid).await?;
        if kind == "ddns" {
            let ttl = value["ttl"].as_u64().unwrap_or(0);
            if credential.provider != "cloudflare" && (ttl < 600 || value["proxied"] == true) {
                return Err(ApiError::bad(
                    "阿里云和 DNSPod 的 TTL 至少为 600 秒，且不支持 Cloudflare 代理",
                ));
            }
            if state.store.list("ddns").await?.iter().any(|r| {
                r["id"] != id
                    && r["domain"] == value["domain"]
                    && r["record_type"] == value["record_type"]
            }) {
                return Err(ApiError::bad("此域名和记录类型已有更新任务"));
            }
        }
    }
    if kind == "web" {
        let listener = value["listener"].as_str().unwrap_or("default");
        if listener != "default"
            && !state
                .config
                .listeners
                .iter()
                .any(|entry| entry.id == listener)
        {
            return Err(ApiError::bad("监听入口未配置，请先配置服务启动环境"));
        }
        let rules = state.store.list("web").await?;
        if rules
            .iter()
            .any(|r| r["id"] != id && r["domain"] == value["domain"])
        {
            return Err(ApiError::bad("此域名已有 Web 服务"));
        }
        let cid = value["certificate_id"].as_str().unwrap_or_default();
        if !cid.is_empty() {
            let cert = state.store.get::<Certificate>("certificates", cid).await?;
            let domain = value["domain"].as_str().unwrap_or_default();
            if !cert
                .domains
                .iter()
                .any(|d| crate::certificates::covers(d, domain))
            {
                return Err(ApiError::bad("证书域名不覆盖此 Web 服务"));
            }
        }
    }
    if kind == "certificates" && update {
        let previous: Certificate = state.store.get(kind, id).await?;
        if serde_json::to_value(previous.domains).unwrap_or_default() != value["domains"]
            || previous.staging != value["staging"].as_bool().unwrap_or(false)
            || previous.imported != value["imported"].as_bool().unwrap_or(false)
        {
            return Err(ApiError::bad("证书域名、环境和来源不能修改，请新建证书"));
        }
    }
    state.store.save(kind, id, value).await?;
    if kind == "ddns" {
        state
            .store
            .status(kind, id, json!({"state":"idle"}))
            .await?;
    }
    crate::proxy::refresh(state).await?;
    state
        .store
        .event(
            "info",
            kind,
            if update {
                "配置已更新"
            } else {
                "配置已创建"
            },
        )
        .await;
    Ok(())
}
pub async fn delete(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    valid_kind(&kind)?;
    let _lock = state.mutations.lock().await;
    if state.jobs.lock().await.contains(&format!("{kind}:{id}")) {
        return Err(ApiError(StatusCode::CONFLICT, "任务正在执行".into()));
    }
    for related in ["ddns", "web", "certificates"] {
        if state.store.list(related).await?.iter().any(|r| {
            (kind == "credentials" && r["credential_id"] == id)
                || (kind == "certificates" && r["certificate_id"] == id)
        }) {
            return Err(ApiError::bad("此资源仍被使用，请先解除关联"));
        }
    }
    let deleted = sqlx::query("DELETE FROM resources WHERE kind=? AND id=?")
        .bind(&kind)
        .bind(&id)
        .execute(&state.store.pool)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "资源不存在".into()));
    }
    crate::proxy::refresh(&state).await?;
    if kind == "certificates" {
        let path = state.config.data_dir.join("certificates").join(&id);
        let _ = tokio::fs::remove_dir_all(path).await;
    }
    state.store.event("info", &kind, "配置已删除").await;
    Ok(Json(json!({"ok":true})))
}
pub async fn run(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if !["ddns", "certificates"].contains(&kind.as_str()) {
        return Err(ApiError::bad("此资源不支持执行任务"));
    }
    state.store.get::<Value>(&kind, &id).await?;
    crate::jobs::enqueue(state, kind, id, true).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({"queued":true}))))
}
#[derive(Deserialize)]
pub struct LogQuery {
    #[serde(default)]
    level: String,
    #[serde(default)]
    module: String,
}
pub async fn logs(
    State(state): State<AppState>,
    Query(query): Query<LogQuery>,
) -> Result<Json<Value>, ApiError> {
    let rows = sqlx::query("SELECT * FROM events WHERE (?='' OR level=?) AND (?='' OR module=?) ORDER BY id DESC LIMIT 300").bind(&query.level).bind(&query.level).bind(&query.module).bind(&query.module).fetch_all(&state.store.pool).await?;
    Ok(Json(json!(rows.into_iter().map(|r| json!({"id":r.get::<i64,_>("id"),"time":r.get::<i64,_>("time"),"level":r.get::<String,_>("level"),"module":r.get::<String,_>("module"),"message":r.get::<String,_>("message")})).collect::<Vec<_>>())))
}
pub async fn overview(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let mut counts = serde_json::Map::new();
    for kind in ["ddns", "web", "certificates"] {
        let resources = checked_list(&state, kind).await?;
        counts.insert(kind.into(),json!({"total":resources.len(),"enabled":resources.iter().filter(|r| r["enabled"]==true || (kind=="certificates" && r["status"]["state"]=="ready")).count(),"errors":resources.iter().filter(|r|r["status"]["state"]=="error").count()}));
    }
    Ok(Json(
        json!({"counts":counts,"uptime":state.started.elapsed().as_secs(),"build_sha":state.config.build_sha,"version":env!("CARGO_PKG_VERSION"),"admin_addr":state.config.admin_addr,"http_addr":state.config.http_addr,"https_addr":state.config.https_addr,"listeners":state.config.listeners}),
    ))
}
pub async fn interfaces() -> Result<Json<Value>, ApiError> {
    let items = if_addrs::get_if_addrs().map_err(|_| ApiError::bad("无法读取网卡"))?;
    Ok(Json(json!(items
        .into_iter()
        .filter(|i| !i.is_loopback())
        .map(|i| json!({"name":i.name,"ip":i.ip().to_string()}))
        .collect::<Vec<_>>())))
}
