// @created 2026-10-13
// @description 校验 Web 协议、TLS、上游超时与自定义头配置。
// @author yunhungo
use anyhow::{ensure, Result};
use axum::http::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebOptions {
    pub access_mode: String,
    pub http1: bool,
    pub http2: bool,
    pub http3: bool,
    pub tls_min: String,
    pub upstream_version: String,
    pub connect_timeout: u64,
    pub response_timeout: u64,
    pub request_headers: BTreeMap<String, Option<String>>,
    pub response_headers: BTreeMap<String, Option<String>>,
}
impl Default for WebOptions {
    fn default() -> Self {
        Self {
            access_mode: "both".into(),
            http1: true,
            http2: true,
            http3: false,
            tls_min: "1.2".into(),
            upstream_version: "auto".into(),
            connect_timeout: 10,
            response_timeout: 60,
            request_headers: BTreeMap::new(),
            response_headers: BTreeMap::new(),
        }
    }
}
impl WebOptions {
    pub fn validate(&self, rule: &crate::models::WebRule) -> Result<()> {
        ensure!(
            ["both", "http", "https"].contains(&self.access_mode.as_str()),
            "请选择有效的接入协议"
        );
        ensure!(
            self.http1 || self.http2 || self.http3,
            "至少启用一种 HTTP 版本"
        );
        ensure!(
            ["1.2", "1.3"].contains(&self.tls_min.as_str()),
            "TLS 最低版本必须为 1.2 或 1.3"
        );
        ensure!(
            ["auto", "http1", "http2"].contains(&self.upstream_version.as_str()),
            "请选择有效的上游 HTTP 版本"
        );
        ensure!(
            (1..=60).contains(&self.connect_timeout),
            "连接超时应为 1–60 秒"
        );
        ensure!(
            (1..=600).contains(&self.response_timeout),
            "响应头超时应为 1–600 秒"
        );
        ensure!(
            !self.http3 || (self.access_mode != "http" && !rule.certificate_id.is_empty()),
            "HTTP/3 需要启用 HTTPS 并绑定证书"
        );
        ensure!(
            self.access_mode != "https" || !rule.certificate_id.is_empty(),
            "仅 HTTPS 接入需要绑定证书"
        );
        ensure!(
            self.access_mode != "http" || (!rule.https_redirect && self.http1),
            "仅 HTTP 接入需要 HTTP/1.1，且不能启用 HTTPS 跳转"
        );
        for headers in [&self.request_headers, &self.response_headers] {
            ensure!(headers.len() <= 32, "每组自定义头最多 32 项");
            let mut names = std::collections::HashSet::new();
            let mut size = 0;
            for (name, value) in headers {
                let parsed = HeaderName::from_bytes(name.as_bytes())?;
                ensure!(names.insert(parsed.clone()), "自定义头名称不能重复");
                ensure!(
                    ![
                        "connection",
                        "upgrade",
                        "content-length",
                        "transfer-encoding",
                        "te",
                        "trailer",
                        "keep-alive",
                        "proxy-connection",
                        "alt-svc",
                        "x-openlucky-via",
                        "x-openlucky-request-id",
                        "forwarded",
                        "x-forwarded-for",
                        "x-forwarded-host",
                        "x-forwarded-proto",
                        "x-real-ip",
                        "authorization",
                        "proxy-authorization",
                        "cookie",
                        "set-cookie"
                    ]
                    .contains(&parsed.as_str()),
                    "此请求头由系统管理或包含凭据，不能在自定义头中配置：{name}"
                );
                if let Some(value) = value {
                    ensure!(value.len() <= 8192, "自定义头值最多 8192 字节");
                    HeaderValue::from_str(value)?;
                    size += name.len() + value.len();
                }
            }
            ensure!(size <= 32768, "自定义头总大小不能超过 32 KiB");
        }
        ensure!(
            !self
                .response_headers
                .keys()
                .any(|key| key.eq_ignore_ascii_case("host")),
            "响应头不能设置 Host"
        );
        crate::header_rules::Rules::compile(&self.request_headers, false)?;
        crate::header_rules::Rules::compile(&self.response_headers, true)?;
        Ok(())
    }
}
