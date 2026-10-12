// @created 2026-10-13
// @description 提供受认证保护的访问分页、统计及采集健康状态接口。
// @author yunhungo
use super::*;
use crate::{error::ApiError, AppState};
use axum::{
    extract::{Query, State},
    Json,
};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row, Sqlite};

#[derive(Deserialize, Default)]
pub struct Filter {
    #[serde(default)]
    service_id: String,
    #[serde(default)]
    errors: bool,
    status: Option<u16>,
    before: Option<i64>,
    since: Option<i64>,
    until: Option<i64>,
    limit: Option<i64>,
    #[serde(default)]
    q: String,
}
impl Filter {
    fn validate(&self) -> Result<(), ApiError> {
        if self.service_id.len() > 64
            || self.q.len() > 200
            || self.status.is_some_and(|v| !(100..=599).contains(&v))
            || self.limit.is_some_and(|v| !(1..=200).contains(&v))
            || self.before.is_some_and(|v| v < 1)
            || self.since.is_some_and(|v| v < 0)
            || self.until.is_some_and(|v| v < 0)
            || matches!((self.since,self.until),(Some(a),Some(b)) if a>b)
        {
            return Err(ApiError::bad("无效的日志筛选条件"));
        }
        Ok(())
    }
}
pub async fn logs(
    State(state): State<AppState>,
    Query(filter): Query<Filter>,
) -> Result<Json<Value>, ApiError> {
    filter.validate()?;
    let limit = filter.limit.unwrap_or(50);
    let mut query = QueryBuilder::<Sqlite>::new("SELECT id,entry FROM access_logs WHERE 1=1");
    if !filter.service_id.is_empty() {
        query.push(" AND service_id=").push_bind(filter.service_id);
    }
    if filter.errors {
        query.push(" AND failed=1");
    }
    if let Some(status) = filter.status {
        query.push(" AND status=").push_bind(status);
    }
    if let Some(before) = filter.before {
        query.push(" AND id<").push_bind(before);
    }
    if let Some(since) = filter.since {
        query.push(" AND time>=").push_bind(since);
    }
    if let Some(until) = filter.until {
        query.push(" AND time<=").push_bind(until);
    }
    if !filter.q.is_empty() {
        query
            .push(" AND (instr(json_extract(entry,'$.path'),")
            .push_bind(filter.q.clone())
            .push(")>0 OR instr(json_extract(entry,'$.client_ip'),")
            .push_bind(filter.q.clone())
            .push(")>0 OR instr(json_extract(entry,'$.request_id'),")
            .push_bind(filter.q.clone())
            .push(")>0 OR instr(json_extract(entry,'$.host'),")
            .push_bind(filter.q)
            .push(")>0)");
    }
    query.push(" ORDER BY id DESC LIMIT ").push_bind(limit + 1);
    let rows = query.build().fetch_all(&state.telemetry.pool).await?;
    let more = rows.len() > limit as usize;
    let mut items = Vec::new();
    for row in rows.into_iter().take(limit as usize) {
        let mut entry: Value =
            serde_json::from_str(row.get("entry")).map_err(anyhow::Error::from)?;
        entry["id"] = json!(row.get::<i64, _>("id"));
        items.push(entry);
    }
    let next = if more {
        items.last().and_then(|v| v["id"].as_i64())
    } else {
        None
    };
    Ok(Json(json!({"items":items,"next_cursor":next})))
}
#[derive(Deserialize)]
pub struct TrafficQuery {
    #[serde(default)]
    service_id: String,
    hours: Option<i64>,
}
pub async fn traffic(
    State(state): State<AppState>,
    Query(query): Query<TrafficQuery>,
) -> Result<Json<Value>, ApiError> {
    let hours = query.hours.unwrap_or(24);
    if ![24, 168, 720].contains(&hours) || query.service_id.len() > 64 {
        return Err(ApiError::bad("无效的统计范围"));
    }
    Ok(Json(
        state.telemetry.traffic(&query.service_id, hours).await?,
    ))
}
pub async fn health(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let telemetry = &state.telemetry;
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM access_logs")
        .fetch_one(&telemetry.pool)
        .await?;
    let pages: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&telemetry.pool)
        .await?;
    let pending = telemetry.pending.lock().unwrap_or_else(|e| e.into_inner());
    Ok(Json(json!({
        "policy":telemetry.policy,"rows":rows,"database_bytes":pages*4096,
        "pending":pending.normal.len()+pending.errors.len(),
        "dropped_logs":telemetry.dropped_logs.load(Ordering::Relaxed),
        "dropped_buckets":telemetry.dropped_buckets.load(Ordering::Relaxed),
        "write_failures":telemetry.write_failures.load(Ordering::Relaxed),
        "last_flush":telemetry.last_flush.load(Ordering::Relaxed),
    })))
}
impl Telemetry {
    pub(super) async fn traffic(&self, service_id: &str, hours: i64) -> Result<Value> {
        let end = chrono::Utc::now().timestamp() / 3600 * 3600;
        let start = end - (hours - 1) * 3600;
        let rows = sqlx::query(
            "SELECT * FROM traffic WHERE service_id=? AND hour>=? AND hour<=? ORDER BY hour",
        )
        .bind(service_id)
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await?;
        let mut buckets: HashMap<i64, Bucket> = rows
            .into_iter()
            .map(|row| {
                (
                    row.get("hour"),
                    Bucket {
                        requests: row.get("requests"),
                        client_errors: row.get("client_errors"),
                        server_errors: row.get("server_errors"),
                        stream_errors: row.get("stream_errors"),
                        latency_ms: row.get("latency_ms"),
                    },
                )
            })
            .collect();
        let mut total = Bucket::default();
        let mut series = Vec::new();
        for hour in (start..=end).step_by(3600) {
            let b = buckets.remove(&hour).unwrap_or_default();
            series.push(json!({"time":hour,"requests":b.requests,"client_errors":b.client_errors,"server_errors":b.server_errors}));
            total.requests += b.requests;
            total.client_errors += b.client_errors;
            total.server_errors += b.server_errors;
            total.stream_errors += b.stream_errors;
            total.latency_ms += b.latency_ms;
        }
        Ok(
            json!({"since":start,"until":end+3600,"requests":total.requests,"client_errors":total.client_errors,"server_errors":total.server_errors,"stream_errors":total.stream_errors,
            "avg_duration_ms":if total.requests>0 {Some(total.latency_ms/total.requests as f64)} else {None},"series":series}),
        )
    }
}
