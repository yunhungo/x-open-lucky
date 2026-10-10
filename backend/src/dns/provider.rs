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
        r.domain.trim_end_matches(&format!(".{}", r.zone)).into()
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
