// @created 2026-10-13
// @description 以原子事务更新分组顺序和服务归属，避免覆盖服务其他配置。
// @author yunhungo
use super::*;
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderInput {
    groups: Vec<VersionedId>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignInput {
    group_id: String,
    services: Vec<VersionedId>,
}

pub async fn reorder(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<OrderInput>,
) -> Result<Json<Value>, ApiError> {
    let _lock = state.mutations.lock().await;
    let current = read(&state.store.pool).await?;
    if body.groups.len() != current.len() || body.groups.len() > MAX_GROUPS {
        return Err(conflict());
    }
    let mut seen = HashSet::new();
    for row in &body.groups {
        if !seen.insert(&row.id)
            || !current
                .iter()
                .any(|g| g.id == row.id && g.revision == row.revision)
        {
            return Err(conflict());
        }
    }
    let mut tx = state.store.pool.begin().await?;
    for (position, row) in body.groups.iter().enumerate() {
        sqlx::query(
            "UPDATE service_groups SET position=?,revision=revision+1 WHERE id=? AND position<>?",
        )
        .bind(position as i64)
        .bind(&row.id)
        .bind(position as i64)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    state.store.event("info", "groups", "分组顺序已更新").await;
    Ok(Json(json!({"ok":true})))
}
pub async fn assign(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<AssignInput>,
) -> Result<Json<Value>, ApiError> {
    if body.services.is_empty() || body.services.len() > 500 || body.group_id.len() > 64 {
        return Err(ApiError::bad("每次请选择 1–500 个服务"));
    }
    let mut seen = HashSet::new();
    if body
        .services
        .iter()
        .any(|r| r.id.is_empty() || r.id.len() > 64 || !seen.insert(&r.id))
    {
        return Err(ApiError::bad("服务列表无效或重复"));
    }
    let _lock = state.mutations.lock().await;
    require(&state.store.pool, &body.group_id).await?;
    let mut tx = state.store.pool.begin().await?;
    for row in &body.services {
        let result = sqlx::query("UPDATE resources SET data=json_set(data,'$.group_id',?),revision=revision+1 WHERE kind='web' AND id=? AND revision=?")
            .bind(&body.group_id).bind(&row.id).bind(row.revision).execute(&mut *tx).await?;
        if result.rows_affected() != 1 {
            return Err(conflict());
        }
    }
    tx.commit().await?;
    state
        .store
        .event(
            "info",
            "groups",
            &format!("已更新 {} 个服务的分组", body.services.len()),
        )
        .await;
    Ok(Json(json!({"ok":true})))
}
