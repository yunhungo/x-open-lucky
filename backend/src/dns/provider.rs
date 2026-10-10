// @created 2026-10-11
// @description 集中注册 DNS 服务商能力与 ACME 凭据映射。
// @author yunhungo
use crate::models::{Credential, Ddns};
use anyhow::{bail, Context, Result};
pub fn secret<'a>(credential: &'a Credential, key: &str) -> Result<&'a str> {
    credential
        .secrets
        .get(key)
        .map(String::as_str)
        .context("缺少 DNS 凭据")
}
pub fn rr(r: &Ddns) -> String {
    if r.domain == r.zone {
        "@".into()
    } else {
        r.domain
            .strip_suffix(&format!(".{}", r.zone))
            .unwrap_or(&r.domain)
            .into()
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn repeated_zone_suffix_keeps_the_full_record_label() {
        let rule:crate::models::Ddns=serde_json::from_value(serde_json::json!({"id":"x","name":"x","enabled":false,"credential_id":"x","zone":"example.com","domain":"nas.example.com.example.com","record_type":"A","source":"static","source_value":"203.0.113.5","interval":300,"ttl":600,"proxied":false})).unwrap();
        assert_eq!(super::rr(&rule), "nas.example.com");
    }
}

#[derive(Clone, Copy)]
pub enum Provider {
    Cloudflare,
    AliDns,
    TencentCloud,
}
impl Provider {
    pub fn parse(code: &str) -> Result<Self> {
        match code {
            "cloudflare" => Ok(Self::Cloudflare),
            "alidns" => Ok(Self::AliDns),
            "tencentcloud" => Ok(Self::TencentCloud),
            _ => bail!("不支持的 DNS 服务商"),
        }
    }
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            Self::Cloudflare => &["token"],
            _ => &["key_id", "key_secret"],
        }
    }
    pub fn acme_env(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Cloudflare => &[("CLOUDFLARE_DNS_API_TOKEN", "token")],
            Self::AliDns => &[
                ("ALICLOUD_ACCESS_KEY", "key_id"),
                ("ALICLOUD_SECRET_KEY", "key_secret"),
            ],
            Self::TencentCloud => &[
                ("TENCENTCLOUD_SECRET_ID", "key_id"),
                ("TENCENTCLOUD_SECRET_KEY", "key_secret"),
            ],
        }
    }
    pub async fn update(
        self,
        client: &reqwest::Client,
        credential: &Credential,
        rule: &Ddns,
        ip: &str,
    ) -> Result<bool> {
        match self {
            Self::Cloudflare => super::cloudflare::update(client, credential, rule, ip).await,
            Self::AliDns => super::alidns::update(client, credential, rule, ip).await,
            Self::TencentCloud => super::tencentcloud::update(client, credential, rule, ip).await,
        }
    }
}
