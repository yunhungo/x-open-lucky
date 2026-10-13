// @created 2026-10-13
// @description 在有界内存中收集访问明细与独立流量聚合并异步落盘。
// @author yunhungo
pub mod api;
mod metrics;
pub mod observation;
mod storage;
#[cfg(test)]
mod tests;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

const NORMAL_QUEUE: usize = 2048;
const ERROR_QUEUE: usize = 512;
const MAX_BUCKETS: usize = 1024;

#[derive(Clone, Serialize, Deserialize)]
pub struct Access {
    pub time: i64,
    pub request_id: String,
    pub service_id: String,
    pub host: String,
    pub listener: String,
    pub client_ip: String,
    pub method: String,
    pub path: String,
    pub status: u16,
    pub duration_ms: f64,
    pub error: String,
    pub phase: String,
    pub tls: bool,
}
impl Access {
    pub fn failure(&self) -> bool {
        self.status >= 400 || !self.error.is_empty()
    }
}
#[derive(Default)]
struct Bucket {
    requests: i64,
    client_errors: i64,
    server_errors: i64,
    stream_errors: i64,
    latency_ms: f64,
}
#[derive(Default)]
struct Pending {
    normal: VecDeque<Access>,
    errors: VecDeque<Access>,
    buckets: HashMap<(i64, String), Bucket>,
    sample_window: Option<std::time::Instant>,
    successes: u32,
}
struct Summary<'a> {
    time: i64,
    service: &'a str,
    status: u16,
    duration_ms: f64,
    stream: bool,
    failed: bool,
}
#[derive(Clone, Serialize)]
pub struct Policy {
    pub retention_days: i64,
    pub max_rows: i64,
    pub database_limit_bytes: u64,
    pub traffic_days: i64,
    pub success_logs_per_second: u32,
}
impl Policy {
    pub fn from_env() -> Result<Self> {
        fn value(name: &str, default: i64, min: i64, max: i64) -> Result<i64> {
            let value = match std::env::var(name) {
                Ok(v) => v.parse()?,
                Err(_) => default,
            };
            anyhow::ensure!(
                (min..=max).contains(&value),
                "{name} must be between {min} and {max}"
            );
            Ok(value)
        }
        Ok(Self {
            retention_days: value("OPENLUCKY_ACCESS_DAYS", 7, 1, 30)?,
            max_rows: value("OPENLUCKY_ACCESS_ROWS", 20000, 1000, 100000)?,
            database_limit_bytes: 64 * 1024 * 1024,
            traffic_days: 30,
            success_logs_per_second: value("OPENLUCKY_ACCESS_SUCCESS_RATE", 200, 1, 10000)? as u32,
        })
    }
}
pub struct Telemetry {
    pool: SqlitePool,
    policy: Policy,
    pending: Mutex<Pending>,
    dropped_logs: AtomicU64,
    sampled_logs: AtomicU64,
    dropped_buckets: AtomicU64,
    write_failures: AtomicU64,
    last_flush: AtomicU64,
    last_prune: AtomicU64,
    performance: Mutex<metrics::Sampler>,
    flush_ready: tokio::sync::Notify,
}
impl Telemetry {
    pub async fn open(root: &Path, policy: Policy) -> Result<Arc<Self>> {
        let pool = storage::open(root, &policy).await?;
        let telemetry = Arc::new(Self {
            pool,
            policy,
            pending: Mutex::default(),
            dropped_logs: AtomicU64::new(0),
            sampled_logs: AtomicU64::new(0),
            dropped_buckets: AtomicU64::new(0),
            write_failures: AtomicU64::new(0),
            last_flush: AtomicU64::new(0),
            last_prune: AtomicU64::new(0),
            performance: Mutex::new(metrics::Sampler::default()),
            flush_ready: tokio::sync::Notify::new(),
        });
        telemetry.flush().await?;
        Ok(telemetry)
    }
    // 单个 UTF-8 字段按字节截断，查询参数和头部从不传到此边界。
    pub fn bounded(value: &str, bytes: usize) -> String {
        let mut end = value.len().min(bytes);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value[..end].chars().filter(|c| !c.is_control()).collect()
    }
    #[cfg(test)]
    fn record(&self, entry: Access) {
        self.collect(
            Summary {
                time: entry.time,
                service: &entry.service_id,
                status: entry.status,
                duration_ms: entry.duration_ms,
                stream: entry.phase != "request",
                failed: entry.failure(),
            },
            || entry.clone(),
        );
    }
    fn collect(&self, summary: Summary<'_>, make_entry: impl FnOnce() -> Access) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let hour = summary.time / 3600 * 3600;
        // 空 service_id 是全站总量；未匹配域名不生成新维度。
        for service in [
            Some(String::new()),
            (!summary.service.is_empty()).then(|| summary.service.to_owned()),
        ]
        .into_iter()
        .flatten()
        {
            let key = (hour, service);
            if !pending.buckets.contains_key(&key) && pending.buckets.len() >= MAX_BUCKETS {
                self.dropped_buckets.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            let bucket = pending.buckets.entry(key).or_default();
            if !summary.stream {
                bucket.requests += 1;
                bucket.client_errors += i64::from((400..500).contains(&summary.status));
                bucket.server_errors += i64::from(summary.status >= 500);
                bucket.latency_ms += summary.duration_ms;
            } else {
                bucket.stream_errors += 1;
            }
        }
        if !summary.failed {
            let now = std::time::Instant::now();
            if pending
                .sample_window
                .is_none_or(|start| now.duration_since(start) >= Duration::from_secs(1))
            {
                pending.sample_window = Some(now);
                pending.successes = 0;
            }
            if pending.successes >= self.policy.success_logs_per_second {
                self.sampled_logs.fetch_add(1, Ordering::Relaxed);
                return;
            }
            pending.successes += 1;
        }
        drop(pending);
        // 分配和清洗不占用聚合锁；限流的成功请求根本不构造明细。
        let mut entry = make_entry();
        entry.host = Self::bounded(&entry.host, 253);
        entry.path = Self::bounded(entry.path.split(['?', '#']).next().unwrap_or("/"), 512);
        entry.method = Self::bounded(&entry.method, 16);
        entry.listener = Self::bounded(&entry.listener, 64);
        entry.error = Self::bounded(&entry.error, 160);
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let (queue, limit) = if entry.failure() {
            (&mut pending.errors, ERROR_QUEUE)
        } else {
            (&mut pending.normal, NORMAL_QUEUE)
        };
        if queue.len() == limit {
            queue.pop_front();
            self.dropped_logs.fetch_add(1, Ordering::Relaxed);
        }
        queue.push_back(entry);
        if pending.normal.len() + pending.errors.len() >= 256 {
            self.flush_ready.notify_one();
        }
    }
    pub fn performance(&self) -> metrics::Performance {
        self.performance
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .current
            .clone()
    }
    pub async fn run(self: Arc<Self>, shutdown: tokio_util::sync::CancellationToken) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut next_flush = tokio::time::Instant::now();
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = interval.tick() => {
                    self.performance.lock().unwrap_or_else(|e| e.into_inner()).sample();
                }
                _ = async {
                    self.flush_ready.notified().await;
                    tokio::time::sleep_until(next_flush).await;
                } => {}
            }
            if self.flush().await.is_err() {
                tracing::warn!("Telemetry write failed; proxy remains available");
                next_flush = tokio::time::Instant::now() + Duration::from_secs(5);
            } else {
                // 限制阈值刷新频率，避免高峰时每批都触发 NAS 磁盘同步。
                next_flush = tokio::time::Instant::now() + Duration::from_secs(1);
            }
        }
    }
    pub async fn close(&self) {
        if self.flush().await.is_err() {
            tracing::warn!("Final telemetry flush failed");
        }
        self.pool.close().await;
    }
}
