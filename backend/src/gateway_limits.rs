// @created 2026-10-13
// @description 校验 NAS 网关的 QUIC 并发、流控及连接数据窗口预算。
// @author yunhungo
use anyhow::{ensure, Context, Result};

#[derive(Clone, Debug)]
pub struct GatewayLimits {
    pub upstream_idle_per_host: usize,
    pub connections: usize,
    pub requests: usize,
    pub streams: u32,
    pub stream_window: u32,
    pub receive_window: u32,
    pub send_window: u64,
}
impl GatewayLimits {
    pub fn from_env() -> Result<Self> {
        Self::parse(|name| std::env::var(name).ok())
    }
    fn parse(get: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let upstream_idle_per_host = get("OPENLUCKY_UPSTREAM_IDLE_PER_HOST")
            .map(|raw| {
                raw.parse::<usize>()
                    .context("Invalid OPENLUCKY_UPSTREAM_IDLE_PER_HOST")
            })
            .transpose()?
            .unwrap_or(256);
        ensure!(
            (1..=1024).contains(&upstream_idle_per_host),
            "OPENLUCKY_UPSTREAM_IDLE_PER_HOST must be between 1 and 1024"
        );
        let value = |suffix: &str, default: u64, min: u64, max: u64| -> Result<u64> {
            let name = format!("OPENLUCKY_H3_{suffix}");
            let value = get(&name)
                .map(|raw| {
                    raw.parse::<u64>()
                        .with_context(|| format!("Invalid {name}"))
                })
                .transpose()?
                .unwrap_or(default);
            ensure!(
                (min..=max).contains(&value),
                "{name} must be between {min} and {max}"
            );
            Ok(value)
        };
        let limits = Self {
            upstream_idle_per_host,
            connections: value("CONNECTIONS", 64, 1, 4096)? as usize,
            requests: value("REQUESTS", 256, 1, 8192)? as usize,
            streams: value("STREAMS", 32, 1, 256)? as u32,
            stream_window: (value("STREAM_WINDOW_KIB", 1024, 64, 65536)? * 1024) as u32,
            receive_window: (value("RECEIVE_WINDOW_KIB", 1024, 64, 65536)? * 1024) as u32,
            send_window: value("SEND_WINDOW_KIB", 2048, 64, 65536)? * 1024,
        };
        let budget = value("WINDOW_BUDGET_MIB", 192, 1, 4096)? * 1024 * 1024;
        ensure!(
            limits.stream_window <= limits.receive_window,
            "HTTP/3 stream window exceeds connection receive window"
        );
        // 只是连接数据窗口预算；协议元数据、HTTP/1/2、TLS 和连接池还需独立余量。
        ensure!(
            limits.connections as u64 * (u64::from(limits.receive_window) + limits.send_window)
                <= budget,
            "HTTP/3 connection windows exceed OPENLUCKY_H3_WINDOW_BUDGET_MIB"
        );
        Ok(limits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_combinations_that_exceed_memory_budget() {
        assert!(GatewayLimits::parse(|_| None).is_ok());
        assert!(GatewayLimits::parse(
            |key| (key == "OPENLUCKY_UPSTREAM_IDLE_PER_HOST").then(|| "0".into())
        )
        .is_err());
        assert!(GatewayLimits::parse(
            |key| (key == "OPENLUCKY_H3_CONNECTIONS").then(|| "256".into())
        )
        .is_err());
        assert!(GatewayLimits::parse(
            |key| (key == "OPENLUCKY_H3_RECEIVE_WINDOW_KIB").then(|| "64".into())
        )
        .is_err());
        assert!(
            GatewayLimits::parse(|key| (key == "OPENLUCKY_H3_REQUESTS").then(|| "0".into()))
                .is_err()
        );
        assert!(
            GatewayLimits::parse(|key| (key == "OPENLUCKY_H3_STREAMS").then(|| "bad".into()))
                .is_err()
        );
        let tuned = GatewayLimits::parse(|key| match key {
            "OPENLUCKY_H3_SEND_WINDOW_KIB" => Some("8192".into()),
            "OPENLUCKY_H3_CONNECTIONS" => Some("16".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(tuned.send_window, 8 * 1024 * 1024);
    }
}
