// @created 2026-10-11
// @description 查询公网或网卡 IP 并更新 Cloudflare、阿里云和 DNSPod 记录。
// @author yunhungo
use crate::{
    models::{Credential, Ddns},
    AppState,
};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::net::IpAddr;

pub async fn resolve_ip(state: &AppState, rule: &Ddns) -> Result<IpAddr> {
    let ip = match rule.source.as_str() {
        "static" => rule.source_value.parse()?,
        "interface" => if_addrs::get_if_addrs()?
            .into_iter()
            .filter(|i| i.name == rule.source_value)
            .map(|i| i.ip())
            .find(|ip| ip.is_ipv4() == (rule.record_type == "A") && usable_ip(*ip))
            .context("网卡没有可用的指定版本 IP")?,
        "public" => {
            let (client, default_url) = if rule.record_type == "A" {
                (&state.ipv4, "https://api.ipify.org")
            } else {
                (&state.ipv6, "https://api6.ipify.org")
            };
            let urls: Vec<_> = if rule.source_value.trim().is_empty() {
                vec![default_url]
            } else {
                rule.source_value.split_whitespace().collect()
            };
            let mut found = None;
            for url in urls {
                if let Ok(ip) = query_ip(client, url, rule.record_type == "A").await {
                    found = Some(ip);
                    break;
                }
            }
            found.context("所有公网 IP 查询接口均失败或返回无效地址")?
        }
        _ => bail!("未知 IP 来源"),
    };
    if ip.is_ipv4() != (rule.record_type == "A") || !usable_ip(ip) {
        bail!("IP 地址类型无效或为本地保留地址");
    }
    Ok(ip)
}
async fn query_ip(client: &reqwest::Client, url: &str, ipv4: bool) -> Result<IpAddr> {
    let mut response = client
        .get(url)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await?
        .error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > 4096 {
            bail!("IP 查询响应过长");
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = std::str::from_utf8(&bytes)?;
    text.split(|c: char| !c.is_ascii_hexdigit() && c != ':' && c != '.')
        .filter_map(|s| s.parse::<IpAddr>().ok())
        .find(|ip| ip.is_ipv4() == ipv4 && usable_ip(*ip))
        .context("查询接口未返回有效 IP")
}
fn usable_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !ip.is_loopback() && !ip.is_unspecified() && !ip.is_multicast() && !ip.is_link_local()
        }
        IpAddr::V6(ip) => {
            !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && (ip.segments()[0] & 0xffc0 != 0xfe80)
        }
    }
}
pub async fn update(state: &AppState, rule: &Ddns) -> Result<Value> {
    let credential: Credential = state.store.get("credentials", &rule.credential_id).await?;
    let ip = resolve_ip(state, rule).await?.to_string();
    let changed = provider::Provider::parse(&credential.provider)?
        .update(&state.http, &credential, rule, &ip)
        .await?;
    Ok(
        json!({"state":"ready","ip":ip,"changed":changed,"last_attempt":chrono::Utc::now().timestamp(),"message":if changed { "记录已更新" } else { "记录已同步" }}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn query_parses_ipv6_and_rejects_wrong_family_and_large_responses() {
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(path("/ip"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("当前 IP：240e:39b:802:56e0:265e:beff:fe6a:cac 中国"),
            )
            .mount(&server)
            .await;
        Mock::given(path("/huge"))
            .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(4097)))
            .mount(&server)
            .await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        assert_eq!(
            query_ip(&client, &format!("{}/ip", server.uri()), false)
                .await
                .unwrap()
                .to_string(),
            "240e:39b:802:56e0:265e:beff:fe6a:cac"
        );
        assert!(query_ip(&client, &format!("{}/ip", server.uri()), true)
            .await
            .is_err());
        assert!(query_ip(&client, &format!("{}/huge", server.uri()), false)
            .await
            .is_err());
    }
    #[test]
    fn local_reserved_addresses_are_not_published() {
        for ip in ["127.0.0.1", "0.0.0.0", "169.254.1.1", "::1", "fe80::1"] {
            assert!(!usable_ip(ip.parse().unwrap()));
        }
        assert!(usable_ip("2606:4700::1111".parse().unwrap()));
    }
}

#[cfg(test)]
mod adapter_tests;
mod alidns;
mod cloudflare;
pub mod provider;
mod tencentcloud;
