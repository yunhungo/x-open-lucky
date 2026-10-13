// @created 2026-10-13
// @description 管理服务分组的持久化、名称校验和受版本保护的 CRUD。
// @author yunhungo
pub mod backup;
mod membership;
pub mod scope;
use crate::{
    error::{ApiError, ApiJson},
    AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
pub use membership::{assign, reorder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;

pub const MAX_GROUPS: usize = 64;
#[derive(Clone, Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub position: i64,
    pub revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameInput {
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameInput {
    name: String,
    revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionInput {
    revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionedId {
    pub id: String,
    pub revision: i64,
}

pub fn name(raw: &str) -> Result<String, ApiError> {
    let value = raw.trim();
    if value.is_empty() || value.chars().count() > 40 || value.chars().any(char::is_control) {
        return Err(ApiError::bad("分组名称应为 1–40 个字符"));
    }
    if value == "未分组" {
        return Err(ApiError::bad("未分组是保留名称"));
    }
    Ok(value.to_owned())
}
pub fn conflict() -> ApiError {
    ApiError(
        StatusCode::CONFLICT,
        "分组或服务已被更新，请刷新后重试".into(),
    )
}
pub async fn read(pool: &SqlitePool) -> Result<Vec<Group>, sqlx::Error> {
    sqlx::query_as("SELECT id,name,position,revision FROM service_groups ORDER BY position,id")
        .fetch_all(pool)
        .await
}
pub async fn require(pool: &SqlitePool, id: &str) -> Result<(), ApiError> {
    if id.is_empty() {
        return Ok(());
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM service_groups WHERE id=?)")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if !exists {
        return Err(ApiError(StatusCode::NOT_FOUND, "分组不存在".into()));
    }
    Ok(())
}
async fn unique(pool: &SqlitePool, name: &str, except: &str) -> Result<(), ApiError> {
    if read(pool)
        .await?
        .iter()
        .any(|g| g.id != except && g.name.to_lowercase() == name.to_lowercase())
    {
        return Err(ApiError::bad("分组名称已存在"));
    }
    Ok(())
}
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Group>>, ApiError> {
    Ok(Json(read(&state.store.pool).await?))
}
pub async fn create(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<NameInput>,
) -> Result<Json<Value>, ApiError> {
    let name = name(&body.name)?;
    let _lock = state.mutations.lock().await;
    let groups = read(&state.store.pool).await?;
    if groups.len() >= MAX_GROUPS {
        return Err(ApiError::bad("最多创建 64 个分组"));
    }
    unique(&state.store.pool, &name, "").await?;
    let id = uuid::Uuid::new_v4().to_string();
    let position = groups.last().map_or(0, |g| g.position + 1);
    sqlx::query("INSERT INTO service_groups(id,name,position) VALUES(?,?,?)")
        .bind(&id)
        .bind(name)
        .bind(position)
        .execute(&state.store.pool)
        .await?;
    state.store.event("info", "groups", "服务分组已创建").await;
    Ok(Json(json!({"id":id})))
}
pub async fn rename(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<RenameInput>,
) -> Result<Json<Value>, ApiError> {
    let name = name(&body.name)?;
    let _lock = state.mutations.lock().await;
    require(&state.store.pool, &id).await?;
    unique(&state.store.pool, &name, &id).await?;
    let result = sqlx::query(
        "UPDATE service_groups SET name=?,revision=revision+1 WHERE id=? AND revision=?",
    )
    .bind(name)
    .bind(&id)
    .bind(body.revision)
    .execute(&state.store.pool)
    .await?;
    if result.rows_affected() != 1 {
        return Err(conflict());
    }
    state.store.event("info", "groups", "服务分组已更新").await;
    Ok(Json(json!({"id":id})))
}
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<RevisionInput>,
) -> Result<Json<Value>, ApiError> {
    let _lock = state.mutations.lock().await;
    require(&state.store.pool, &id).await?;
    let mut tx = state.store.pool.begin().await?;
    let result = sqlx::query("DELETE FROM service_groups WHERE id=? AND revision=?")
        .bind(&id)
        .bind(body.revision)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() != 1 {
        return Err(conflict());
    }
    let members = sqlx::query("UPDATE resources SET data=json_set(data,'$.group_id',''),revision=revision+1 WHERE kind='web' AND json_extract(data,'$.group_id')=?")
        .bind(&id).execute(&mut *tx).await?.rows_affected();
    tx.commit().await?;
    state
        .store
        .event("info", "groups", "分组已删除，成员已转为未分组")
        .await;
    Ok(Json(json!({"ok":true,"moved":members})))
}
