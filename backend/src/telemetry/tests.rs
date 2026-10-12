// @created 2026-10-13
// @description 验证访问队列降载、独立统计、清理、脱敏和存储页上限。
// @author yunhungo
use super::*;
fn policy(rows: i64) -> Policy {
    Policy {
        retention_days: 7,
        max_rows: rows,
        database_limit_bytes: 64 * 1024 * 1024,
        traffic_days: 30,
    }
}
fn entry(status: u16) -> Access {
    Access {
        time: chrono::Utc::now().timestamp(),
        request_id: uuid::Uuid::new_v4().to_string(),
        service_id: "service-a".into(),
        host: "example.test".into(),
        listener: "default".into(),
        client_ip: "127.0.0.1".into(),
        method: "GET".into(),
        path: "/hello?token=secret#private".into(),
        status,
        duration_ms: 12.5,
        error: String::new(),
        phase: "request".into(),
        tls: false,
    }
}
#[tokio::test]
async fn overload_keeps_statistics_and_separate_error_capacity() {
    let dir = tempfile::tempdir().unwrap();
    let telemetry = Telemetry::open(dir.path(), policy(5000)).await.unwrap();
    for _ in 0..3000 {
        telemetry.record(entry(200));
    }
    for _ in 0..600 {
        telemetry.record(entry(502));
    }
    {
        let pending = telemetry.pending.lock().unwrap();
        assert_eq!(pending.normal.len(), NORMAL_QUEUE);
        assert_eq!(pending.errors.len(), ERROR_QUEUE);
        assert!(pending.normal.iter().all(|e| e.path == "/hello"));
    }
    assert_eq!(telemetry.dropped_logs.load(Ordering::Relaxed), 1040);
    telemetry.flush().await.unwrap();
    let stats = telemetry.traffic("service-a", 24).await.unwrap();
    assert_eq!(stats["requests"], 3600);
    assert_eq!(stats["server_errors"], 600);
    assert_eq!(stats["avg_duration_ms"], 12.5);
    assert_eq!(telemetry.traffic("", 24).await.unwrap()["requests"], 3600);
    assert_eq!(
        telemetry.traffic("missing", 24).await.unwrap()["requests"],
        0
    );
    let errors: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM access_logs WHERE failed=1")
        .fetch_one(&telemetry.pool)
        .await
        .unwrap();
    assert_eq!(errors, ERROR_QUEUE as i64);
}
#[tokio::test]
async fn retention_and_row_limit_preserve_recent_data_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let telemetry = Telemetry::open(dir.path(), policy(3)).await.unwrap();
    for _ in 0..10 {
        telemetry.record(entry(200));
    }
    let mut stream_error = entry(200);
    stream_error.phase = "stream".into();
    stream_error.error = "响应流中断".into();
    telemetry.record(stream_error);
    telemetry.flush().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM access_logs")
        .fetch_one(&telemetry.pool)
        .await
        .unwrap();
    assert_eq!(count, 3);
    assert_eq!(telemetry.traffic("", 24).await.unwrap()["requests"], 10);
    assert_eq!(telemetry.traffic("", 24).await.unwrap()["stream_errors"], 1);
    sqlx::query("UPDATE access_logs SET time=0 WHERE id=(SELECT MIN(id) FROM access_logs)")
        .execute(&telemetry.pool)
        .await
        .unwrap();
    telemetry.close().await;
    let reopened = Telemetry::open(dir.path(), policy(3)).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM access_logs")
        .fetch_one(&reopened.pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
    assert_eq!(reopened.traffic("", 24).await.unwrap()["requests"], 10);
    let limit: i64 = sqlx::query_scalar("PRAGMA max_page_count")
        .fetch_one(&reopened.pool)
        .await
        .unwrap();
    assert_eq!(limit * 4096, 64 * 1024 * 1024);
}
#[tokio::test]
async fn high_cardinality_has_bounded_memory_and_global_counts() {
    let dir = tempfile::tempdir().unwrap();
    let telemetry = Telemetry::open(dir.path(), policy(20)).await.unwrap();
    for n in 0..2000 {
        let mut record = entry(200);
        record.service_id = format!("service-{n}");
        telemetry.record(record);
    }
    assert_eq!(telemetry.pending.lock().unwrap().buckets.len(), MAX_BUCKETS);
    assert!(telemetry.dropped_buckets.load(Ordering::Relaxed) > 0);
    telemetry.flush().await.unwrap();
    assert_eq!(telemetry.traffic("", 24).await.unwrap()["requests"], 2000);
}
#[test]
fn truncation_keeps_valid_unicode_and_removes_control_characters() {
    assert_eq!(Telemetry::bounded("中文中文", 7), "中文");
    assert_eq!(Telemetry::bounded("/test\r\nnext", 100), "/testnext");
    let fields = [
        "S", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "100", "50",
    ];
    assert_eq!(
        metrics::cpu_ticks(&format!("12 (process (name)) {}", fields.join(" "))),
        Some(150)
    );
}

#[tokio::test]
async fn full_database_reports_loss_and_recovers_without_unbounded_retry() {
    let dir = tempfile::tempdir().unwrap();
    let telemetry = Telemetry::open(
        dir.path(),
        Policy {
            database_limit_bytes: 128 * 1024,
            ..policy(5000)
        },
    )
    .await
    .unwrap();
    for _ in 0..1000 {
        let mut record = entry(200);
        record.path = format!("/{}", "x".repeat(500));
        telemetry.record(record);
    }
    assert!(telemetry.flush().await.is_err());
    assert_eq!(telemetry.dropped_logs.load(Ordering::Relaxed), 1000);
    assert!(telemetry.dropped_buckets.load(Ordering::Relaxed) > 0);
    assert_eq!(telemetry.write_failures.load(Ordering::Relaxed), 1);
    assert!(telemetry.pending.lock().unwrap().normal.is_empty());
    telemetry.record(entry(502));
    telemetry.flush().await.unwrap();
    assert_eq!(telemetry.traffic("", 24).await.unwrap()["requests"], 1);
    let pages: u64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&telemetry.pool)
        .await
        .unwrap();
    assert!(pages * 4096 <= 128 * 1024);
}
