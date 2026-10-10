// @created 2026-10-11
// @description 定义资源合同并校验域名、凭据与代理配置。
// @author yunhungo
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
pub struct Credential {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub secrets: BTreeMap<String, String>,
    #[serde(default)]
    pub has_secret: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ddns {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub credential_id: String,
    pub zone: String,
    pub domain: String,
    pub record_type: String,
    pub source: String,
    #[serde(default)]
    pub source_value: String,
    pub interval: u64,
    pub ttl: u32,
    #[serde(default)]
    pub proxied: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WebRule {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub domain: String,
    pub target: String,
    #[serde(default)]
    pub certificate_id: String,
    #[serde(default)]
    pub https_redirect: bool,
    #[serde(default)]
    pub preserve_host: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Certificate {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub domains: Vec<String>,
    pub email: String,
    pub credential_id: String,
    pub staging: bool,
    pub auto_renew: bool,
    #[serde(default)]
    pub imported: bool,
}

pub fn domain_valid(domain: &str, wildcard: bool) -> bool {
    let domain = if wildcard {
        domain.strip_prefix("*.").unwrap_or(domain)
    } else {
        domain
    };
    domain.len() <= 253
        && domain.contains('.')
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

pub fn validate(kind: &str, mut value: Value) -> Result<Value> {
    for field in ["domain", "zone"] {
        if let Some(text) = value[field].as_str() {
            value[field] = Value::String(text.trim().to_ascii_lowercase());
        }
    }
    if let Some(text) = value["name"].as_str() {
        value["name"] = Value::String(text.trim().to_owned());
    }
    if let Some(domains) = value["domains"].as_array_mut() {
        for domain in domains {
            if let Some(text) = domain.as_str() {
                *domain = Value::String(text.trim().to_ascii_lowercase());
            }
        }
    }
    if value["name"].as_str().is_none_or(|n| {
        n.trim().is_empty() || n.chars().count() > 100 || n.chars().any(char::is_control)
    }) {
        bail!("名称应为 1–100 个字符");
    }
    match kind {
        "credentials" => {
            let item: Credential = serde_json::from_value(value)?;
            if !["cloudflare", "alidns", "tencentcloud"].contains(&item.provider.as_str()) {
                bail!("不支持的 DNS 服务商");
            }
            Ok(serde_json::to_value(item)?)
        }
        "ddns" => {
            let item: Ddns = serde_json::from_value(value)?;
            if !domain_valid(&item.domain, false) || !domain_valid(&item.zone, false) {
                bail!("请输入有效域名");
            }
            if item.domain != item.zone && !item.domain.ends_with(&format!(".{}", item.zone)) {
                bail!("记录必须属于所选主域名");
            }
            if !["A", "AAAA"].contains(&item.record_type.as_str()) {
                bail!("记录类型只能为 A 或 AAAA");
            }
            if !(60..=86400).contains(&item.interval) {
                bail!("更新间隔应为 60–86400 秒");
            }
            if item.ttl != 1 && !(60..=86400).contains(&item.ttl) {
                bail!("TTL 应为 1（自动）或 60–86400 秒");
            }
            match item.source.as_str() {
                "public" => (),
                "static" => {
                    let ip: std::net::IpAddr = item
                        .source_value
                        .parse()
                        .map_err(|_| anyhow::anyhow!("请输入有效 IP 地址"))?;
                    if ip.is_ipv4() != (item.record_type == "A") {
                        bail!("IP 版本与记录类型不一致");
                    }
                }
                "interface" if !item.source_value.is_empty() => (),
                _ => bail!("请选择有效的 IP 来源"),
            }
            Ok(serde_json::to_value(item)?)
        }
        "web" => {
            let item: WebRule = serde_json::from_value(value)?;
            if !domain_valid(&item.domain, false) {
                bail!("请输入有效域名");
            }
            let target = url::Url::parse(&item.target)
                .map_err(|_| anyhow::anyhow!("请输入完整的目标 URL"))?;
            if !["http", "https"].contains(&target.scheme())
                || target.host_str().is_none()
                || !target.username().is_empty()
                || target.password().is_some()
                || target.fragment().is_some()
                || target.query().is_some()
            {
                bail!("目标仅支持无凭据、查询或片段的 HTTP/HTTPS URL");
            }
            if item.https_redirect && item.certificate_id.is_empty() {
                bail!("HTTPS 跳转需要绑定证书");
            }
            Ok(serde_json::to_value(item)?)
        }
        "certificates" => {
            let item: Certificate = serde_json::from_value(value)?;
            if item.domains.is_empty()
                || item.domains.len() > 20
                || item.domains.iter().any(|d| !domain_valid(d, true))
            {
                bail!("请输入 1–20 个有效证书域名");
            }
            if !item.imported && (!item.email.contains('@') || item.email.len() > 254) {
                bail!("请输入 ACME 联系邮箱");
            }
            Ok(serde_json::to_value(item)?)
        }
        _ => bail!("未知资源类型"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn domain_rejects_injection_and_invalid_labels() {
        assert!(domain_valid("nas.example.com", false));
        assert!(domain_valid("*.example.com", true));
        for domain in [
            "a..com",
            "-a.com",
            "a-.com",
            "a.com/evil",
            "a.com:80",
            "*.a.com",
            "a.com\n",
        ] {
            assert!(!domain_valid(domain, false), "{domain}");
        }
    }
}
