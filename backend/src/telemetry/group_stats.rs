// @created 2026-10-13
// @description 批量汇总当前分组成员的服务状态、访问量与加权响应耗时。
// @author yunhungo
use super::Bucket;
use crate::{error::ApiError, groups, AppState};
use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct QueryRange {
    hours: Option<i64>,
}
struct Summary {
    group: groups::Group,
    services: u64,
    enabled: u64,
    errors: u64,
    traffic: Bucket,
}
impl Summary {
    fn new(group: groups::Group) -> Self {
        Self {
            group,
            services: 0,
            enabled: 0,
            errors: 0,
            traffic: Bucket::default(),
        }
    }
    fn json(self) -> Value {
        let b = self.traffic;
        json!({"id":self.group.id,"name":self.group.name,"position":self.group.position,
            "services":self.services,"enabled":self.enabled,"errors":self.errors,
            "requests":b.requests,"client_errors":b.client_errors,"server_errors":b.server_errors,"stream_errors":b.stream_errors,
            "avg_duration_ms":if b.requests>0 {Some(b.latency_ms/b.requests as f64)} else {None}})
    }
}
pub async fn group_stats(
    State(state): State<AppState>,
    Query(range): Query<QueryRange>,
) -> Result<Json<Value>, ApiError> {
    let hours = range.hours.unwrap_or(24);
    if ![24, 168, 720].contains(&hours) {
        return Err(ApiError::bad("无效的统计范围"));
    }
    let (groups, services) = {
        let _lock = state.mutations.lock().await;
        (
            groups::read(&state.store.pool).await?,
            crate::resources::checked_list(&state, "web").await?,
        )
    };
    let mut summaries = groups.into_iter().map(Summary::new).collect::<Vec<_>>();
    let unassigned = summaries.len();
    summaries.push(Summary::new(groups::Group {
        id: "unassigned".into(),
        name: "未分组".into(),
        position: unassigned as i64,
        revision: 0,
    }));
    let positions: HashMap<_, _> = summaries
        .iter()
        .enumerate()
        .map(|(i, s)| (s.group.id.clone(), i))
        .collect();
    let mut members = HashMap::new();
    for service in services {
        let group = service["group_id"].as_str().unwrap_or_default();
        let position = *positions.get(group).unwrap_or(&unassigned);
        let summary = &mut summaries[position];
        summary.services += 1;
        summary.enabled += u64::from(service["enabled"] == true);
        summary.errors += u64::from(service["status"]["state"] == "error");
        if let Some(id) = service["id"].as_str() {
            members.insert(id.to_owned(), position);
        }
    }
    let end = chrono::Utc::now().timestamp() / 3600 * 3600;
    let start = end - (hours - 1) * 3600;
    if !members.is_empty() {
        let mut query = QueryBuilder::<Sqlite>::new("SELECT service_id,SUM(requests) requests,SUM(client_errors) client_errors,SUM(server_errors) server_errors,SUM(stream_errors) stream_errors,SUM(latency_ms) latency_ms FROM traffic WHERE hour>=");
        query.push_bind(start).push(" AND hour<=").push_bind(end);
        groups::scope::Scope::Services(members.keys().cloned().collect()).push(&mut query);
        query.push(" GROUP BY service_id");
        for row in query.build().fetch_all(&state.telemetry.pool).await? {
            if let Some(position) = members.get(row.get::<&str, _>("service_id")) {
                let b = &mut summaries[*position].traffic;
                b.requests += row.get::<i64, _>("requests");
                b.client_errors += row.get::<i64, _>("client_errors");
                b.server_errors += row.get::<i64, _>("server_errors");
                b.stream_errors += row.get::<i64, _>("stream_errors");
                b.latency_ms += row.get::<f64, _>("latency_ms");
            }
        }
    }
    Ok(Json(
        json!({"since":start,"until":end+3600,"groups":summaries.into_iter().map(Summary::json).collect::<Vec<_>>()}),
    ))
}
