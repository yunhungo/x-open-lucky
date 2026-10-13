// @created 2026-10-13
// @description 将当前分组成员转换为有界查询参数，区分未分组和全局流量。
// @author yunhungo
use crate::{error::ApiError, AppState};
use sqlx::{QueryBuilder, Sqlite};

pub enum Scope {
    All,
    Services(Vec<String>),
}
impl Scope {
    pub fn push<'a>(&self, query: &mut QueryBuilder<'a, Sqlite>) {
        if let Self::Services(ids) = self {
            query
                .push(" AND service_id IN (SELECT value FROM json_each(")
                .push_bind(serde_json::to_string(ids).expect("String list is serializable"))
                .push("))");
        }
    }
}
pub async fn resolve(
    state: &AppState,
    service: &str,
    group: Option<&str>,
) -> Result<Scope, ApiError> {
    if service.len() > 64 || group.is_some_and(|id| id.len() > 64) {
        return Err(ApiError::bad("无效的分组或服务范围"));
    }
    let Some(group) = group.filter(|id| !id.is_empty()) else {
        return Ok(if service.is_empty() {
            Scope::All
        } else {
            Scope::Services(vec![service.into()])
        });
    };
    let _lock = state.mutations.lock().await;
    let id = if group == "unassigned" { "" } else { group };
    super::require(&state.store.pool, id).await?;
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM resources WHERE kind='web' AND COALESCE(json_extract(data,'$.group_id'),'')=? AND (?='' OR id=?)")
        .bind(id).bind(service).bind(service).fetch_all(&state.store.pool).await?;
    Ok(Scope::Services(ids))
}
