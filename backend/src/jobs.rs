// @created 2026-10-11
// @description 去重执行 DDNS 与证书任务，定期重试并记录真实结果。
// @author yunhungo
use crate::{
    error::ApiError,
    models::{Certificate, Ddns},
    AppState,
};
use axum::http::StatusCode;
use serde_json::json;

pub async fn enqueue(
    state: AppState,
    kind: String,
    id: String,
    manual: bool,
) -> Result<(), ApiError> {
    let _lock = state.mutations.lock().await;
    if state.shutdown.is_cancelled() {
        return Err(ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "服务正在停止".into(),
        ));
    }
    let permit = state.job_limit.clone().try_acquire_owned().map_err(|_| {
        ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "后台任务繁忙，请稍后重试".into(),
        )
    })?;
    let acme_permit = if kind == "certificates" {
        Some(state.acme_limit.clone().try_acquire_owned().map_err(|_| {
            ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "其他证书任务正在执行，请稍后重试".into(),
            )
        })?)
    } else {
        None
    };
    let previous = state
        .store
        .list(&kind)
        .await?
        .into_iter()
        .find(|v| v["id"] == id)
        .map(|v| v["status"].clone())
        .ok_or_else(|| ApiError::bad("资源不存在"))?;
    let key = format!("{kind}:{id}");
    {
        let mut jobs = state.jobs.lock().await;
        if !jobs.insert(key.clone()) {
            return Err(ApiError(StatusCode::CONFLICT, "任务已在执行".into()));
        }
    }
    let mut running = previous.clone();
    running["state"] = json!("running");
    running["last_attempt"] = json!(chrono::Utc::now().timestamp());
    if let Err(error) = state.store.status(&kind, &id, running).await {
        state.jobs.lock().await.remove(&key);
        return Err(error.into());
    }
    let task_state = state.clone();
    state.tasks.spawn(async move {
        let state = task_state;
        let _permit = permit;
        let _acme_permit = acme_permit;
        let result = tokio::select! {
            result=execute(&state,&kind,&id,manual)=>result,
            _=state.shutdown.cancelled()=>Err(anyhow::anyhow!("服务停止，任务将按计划重试"))
        };
        match result {
            Ok(status) => {
                let _ = state.store.status(&kind, &id, status).await;
                let _ = crate::proxy::refresh(&state).await;
                state
                    .store
                    .event(
                        "info",
                        &kind,
                        if kind == "ddns" {
                            "DNS 记录已同步"
                        } else {
                            "证书已加载"
                        },
                    )
                    .await;
            }
            Err(error) => {
                let message = error.to_string();
                let mut failed = previous;
                failed["state"] = json!("error");
                failed["message"] = json!(message);
                failed["last_attempt"] = json!(chrono::Utc::now().timestamp());
                let _ = state.store.status(&kind, &id, failed).await;
                state.store.event("error", &kind, &message).await;
            }
        }
        state.jobs.lock().await.remove(&key);
    });
    Ok(())
}
async fn execute(
    state: &AppState,
    kind: &str,
    id: &str,
    manual: bool,
) -> anyhow::Result<serde_json::Value> {
    if kind == "ddns" {
        let rule: Ddns = state.store.get(kind, id).await?;
        crate::dns::update(state, &rule).await
    } else {
        let cert: Certificate = state.store.get(kind, id).await?;
        crate::certificates::issue(state, &cert, manual).await
    }
}
pub async fn scheduler(state: AppState) {
    let mut timer = tokio::time::interval(std::time::Duration::from_secs(2));
    loop {
        tokio::select! {_=timer.tick()=>(),_=state.shutdown.cancelled()=>break};
        let now = chrono::Utc::now().timestamp();
        for kind in ["ddns", "certificates"] {
            let Ok(items) = state.store.list(kind).await else {
                continue;
            };
            for item in items {
                let last = item["status"]["last_attempt"].as_i64().unwrap_or(0);
                let due = if kind == "ddns" {
                    item["enabled"] == true
                        && now - last >= item["interval"].as_i64().unwrap_or(300)
                } else {
                    item["auto_renew"] == true
                        && item["imported"] != true
                        && now - last >= 3600
                        && item["status"]["expires_at"].as_i64().unwrap_or(0) - now < 30 * 86400
                };
                if due {
                    if let Some(id) = item["id"].as_str() {
                        let _ = enqueue(state.clone(), kind.into(), id.into(), false).await;
                    }
                }
            }
        }
        let _ = sqlx::query("DELETE FROM sessions WHERE expires<=?")
            .bind(now)
            .execute(&state.store.pool)
            .await;
    }
}
