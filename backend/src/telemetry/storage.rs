// @created 2026-10-13
// @description 管理独立限容 SQLite、访问批量事务和按时间及条数清理。
// @author yunhungo
use super::*;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

pub(super) async fn open(root: &Path, policy: &Policy) -> Result<SqlitePool> {
    // DELETE journal 避免读者阻止 WAL 回收；批量事务每五秒一次且与配置数据库隔离。
    let options = SqliteConnectOptions::new()
        .filename(root.join("telemetry.sqlite"))
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Delete)
        .pragma("page_size", "4096")
        .pragma(
            "max_page_count",
            (policy.database_limit_bytes / 4096).to_string(),
        )
        .busy_timeout(Duration::from_secs(2));
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./telemetry_migrations").run(&pool).await?;
    Ok(pool)
}
impl Telemetry {
    async fn prune(&self) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM access_logs WHERE time < ? OR id < COALESCE((SELECT id FROM access_logs ORDER BY id DESC LIMIT 1 OFFSET ?),0)")
            .bind(now - self.policy.retention_days * 86400).bind(self.policy.max_rows - 1).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM traffic WHERE hour < ? OR rowid IN (SELECT rowid FROM traffic ORDER BY hour DESC LIMIT -1 OFFSET 100000)")
            .bind(now - self.policy.traffic_days * 86400).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(super) async fn flush(&self) -> Result<()> {
        // 先清理再取队列，清理失败时内存仍受环形队列上限约束。
        if let Err(error) = self.prune().await {
            self.write_failures.fetch_add(1, Ordering::Relaxed);
            return Err(error);
        }
        let pending = std::mem::take(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()));
        let log_count = pending.normal.len() + pending.errors.len();
        let bucket_count = pending.buckets.len();
        let result = self.persist(pending).await;
        if result.is_err() {
            self.dropped_logs
                .fetch_add(log_count as u64, Ordering::Relaxed);
            self.dropped_buckets
                .fetch_add(bucket_count as u64, Ordering::Relaxed);
            self.write_failures.fetch_add(1, Ordering::Relaxed);
            // 磁盘达到页上限时回收最旧的半数诊断数据，下一次写入可自行恢复。
            let _ = sqlx::query("DELETE FROM access_logs WHERE id IN (SELECT id FROM access_logs ORDER BY id LIMIT (SELECT COUNT(*)/2 FROM access_logs))").execute(&self.pool).await;
            let _ = sqlx::query("DELETE FROM traffic WHERE rowid IN (SELECT rowid FROM traffic ORDER BY hour LIMIT (SELECT COUNT(*)/2 FROM traffic))").execute(&self.pool).await;
        } else {
            self.last_flush
                .store(chrono::Utc::now().timestamp() as u64, Ordering::Relaxed);
        }
        result
    }
    async fn persist(&self, pending: Pending) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for ((hour, service), bucket) in pending.buckets {
            sqlx::query("INSERT INTO traffic(hour,service_id,requests,client_errors,server_errors,stream_errors,latency_ms) VALUES(?,?,?,?,?,?,?) ON CONFLICT(hour,service_id) DO UPDATE SET requests=requests+excluded.requests,client_errors=client_errors+excluded.client_errors,server_errors=server_errors+excluded.server_errors,stream_errors=stream_errors+excluded.stream_errors,latency_ms=latency_ms+excluded.latency_ms")
                .bind(hour).bind(service).bind(bucket.requests).bind(bucket.client_errors).bind(bucket.server_errors).bind(bucket.stream_errors).bind(bucket.latency_ms).execute(&mut *tx).await?;
        }
        let mut entries: Vec<_> = pending.normal.into_iter().chain(pending.errors).collect();
        entries.sort_by_key(|e| e.time);
        for entry in entries {
            sqlx::query(
                "INSERT INTO access_logs(time,service_id,status,failed,entry) VALUES(?,?,?,?,?)",
            )
            .bind(entry.time)
            .bind(&entry.service_id)
            .bind(entry.status)
            .bind(entry.failure())
            .bind(serde_json::to_string(&entry)?)
            .execute(&mut *tx)
            .await?;
        }
        // 限制提交后的行数，包含当次批次；高峰不出现无限增长窗口。
        sqlx::query("DELETE FROM access_logs WHERE id < COALESCE((SELECT id FROM access_logs ORDER BY id DESC LIMIT 1 OFFSET ?),0)")
            .bind(self.policy.max_rows - 1).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM traffic WHERE rowid IN (SELECT rowid FROM traffic ORDER BY hour DESC LIMIT -1 OFFSET 100000)").execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
