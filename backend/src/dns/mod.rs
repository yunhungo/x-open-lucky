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
            let (client, url) = if rule.record_type == "A" {
                (&state.ipv4, "https://api.ipify.org")
            } else {
                (&state.ipv6, "https://api6.ipify.org")
            };
            let raw = client
                .get(url)
                .send()
                .await
                .context("公网 IP 查询连接失败")?
                .error_for_status()
                .context("公网 IP 查询失败")?
                .text()
                .await?;
            raw.trim().parse().context("IP 查询返回无效地址")?
        }
        _ => bail!("未知 IP 来源"),
    };
    if ip.is_ipv4() != (rule.record_type == "A") || !usable_ip(ip) {
        bail!("IP 地址类型无效或为本地保留地址");
    }
    Ok(ip)
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
