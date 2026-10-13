// @created 2026-10-13
// @description 对固定本地或授权目标采样并发 HTTP 吞吐与延迟。
// @author yunhungo
use anyhow::Result;
use serde::Deserialize;
use std::{
    io::Read,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Options {
    address: std::net::SocketAddr,
    domain: String,
    ca: Option<String>,
    version: String,
    path: String,
    requests: usize,
    concurrency: usize,
}
#[tokio::main]
async fn main() -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let options: Options = serde_json::from_str(&input)?;
    anyhow::ensure!(
        options.concurrency > 0 && options.concurrency <= 128 && options.requests > 0,
        "Invalid workload"
    );
    let secure = options.ca.is_some();
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .resolve(&options.domain, options.address)
        .pool_max_idle_per_host(options.concurrency);
    builder = if options.version == "http2" {
        builder.http2_prior_knowledge()
    } else {
        builder.http1_only()
    };
    if let Some(ca) = options.ca {
        builder =
            builder.add_root_certificate(reqwest::Certificate::from_pem(&std::fs::read(ca)?)?);
    }
    let client = builder.build()?;
    let url = format!(
        "{}://{}:{}{}",
        if secure { "https" } else { "http" },
        options.domain,
        options.address.port(),
        options.path
    );
    let mut tasks = tokio::task::JoinSet::new();
    let start = Instant::now();
    let mut latencies = Vec::with_capacity(options.requests);
    let mut failures = 0;
    let mut bytes = 0u64;
    for index in 0..options.requests {
        let client = client.clone();
        let url = url.clone();
        let h2 = options.version == "http2";
        tasks.spawn(async move {
            let start = Instant::now();
            let mut response = client.get(url).send().await?;
            let failed = !response.status().is_success()
                || (h2 && response.version() != reqwest::Version::HTTP_2);
            let mut count = 0;
            while let Some(chunk) = response.chunk().await? {
                count += chunk.len() as u64;
            }
            Ok::<_, reqwest::Error>((start.elapsed().as_secs_f64() * 1000.0, failed, count))
        });
        while tasks.len() >= options.concurrency
            || (index + 1 == options.requests && !tasks.is_empty())
        {
            if let Some(result) = tasks.join_next().await {
                match result? {
                    Ok((ms, failed, count)) => {
                        latencies.push(ms);
                        failures += usize::from(failed);
                        bytes += count;
                    }
                    Err(_) => failures += 1,
                }
            }
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    latencies.sort_by(f64::total_cmp);
    let percentile = |p: usize| {
        latencies
            .get((latencies.len() * p / 100).min(latencies.len().saturating_sub(1)))
            .copied()
            .unwrap_or_default()
    };
    println!(
        "{}",
        serde_json::json!({"requests":options.requests,"failures":failures,"seconds":elapsed,"rps":options.requests as f64 / elapsed,"bytes":bytes,"p50_ms":percentile(50),"p95_ms":percentile(95),"p99_ms":percentile(99)})
    );
    Ok(())
}
