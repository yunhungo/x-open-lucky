// @created 2026-10-11
// @description 实现阿里云 DNS RPC 认证与记录更新。
// @author yunhungo
use super::provider::rr;
use super::provider::secret;
use crate::models::{Credential, Ddns};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
use serde_json::Value;
use std::collections::BTreeMap;
fn percent(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
        .replace('*', "%2A")
        .replace("%7E", "~")
}
async fn ali_request(
    client: &reqwest::Client,
    c: &Credential,
    action: &str,
    params: BTreeMap<String, String>,
    endpoint: &str,
) -> Result<Value> {
    let mut params = params;
    for (k, v) in [
        ("Action", action.to_owned()),
        ("AccessKeyId", secret(c, "key_id")?.to_owned()),
        ("Format", "JSON".into()),
        ("Version", "2015-01-09".into()),
        ("SignatureMethod", "HMAC-SHA1".into()),
        ("SignatureVersion", "1.0".into()),
        ("SignatureNonce", uuid::Uuid::new_v4().to_string()),
        (
            "Timestamp",
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        ),
    ] {
        params.insert(k.into(), v);
    }
    let canonical = params
        .iter()
        .map(|(k, v)| format!("{}={}", percent(k), percent(v)))
        .collect::<Vec<_>>()
        .join("&");
    let string = format!("GET&%2F&{}", percent(&canonical));
    let mut mac =
        Hmac::<sha1::Sha1>::new_from_slice(format!("{}&", secret(c, "key_secret")?).as_bytes())?;
    mac.update(string.as_bytes());
    params.insert(
        "Signature".into(),
        STANDARD.encode(mac.finalize().into_bytes()),
    );
    let response = client
        .get(endpoint)
        .query(&params)
        .send()
        .await
        .context("阿里云 DNS 连接失败")?;
    let success = response.status().is_success();
    let value: Value = response.json().await.context("阿里云 DNS 返回无效数据")?;
    if !success || value.get("Code").is_some() {
        bail!(
            "阿里云 DNS 请求失败：{}",
            value["Code"].as_str().unwrap_or("HTTP error")
        );
    }
    Ok(value)
}
pub async fn update(client: &reqwest::Client, c: &Credential, r: &Ddns, ip: &str) -> Result<bool> {
    update_at(client, c, r, ip, "https://alidns.aliyuncs.com/").await
}
pub(super) async fn update_at(
    client: &reqwest::Client,
    c: &Credential,
    r: &Ddns,
    ip: &str,
    endpoint: &str,
) -> Result<bool> {
    let name = rr(r);
    let ttl = r.ttl.max(600);
    let records = ali_request(
        client,
        c,
        "DescribeDomainRecords",
        BTreeMap::from([
            ("DomainName".into(), r.zone.clone()),
            ("RRKeyWord".into(), name.clone()),
            ("TypeKeyWord".into(), r.record_type.clone()),
            ("SearchMode".into(), "EXACT".into()),
            ("PageSize".into(), "500".into()),
        ]),
        endpoint,
    )
    .await?;
    let records = records["DomainRecords"]["Record"]
        .as_array()
        .context("阿里云 DNS 返回记录格式无效")?;
    let matches = records
        .iter()
        .filter(|v| v["RR"] == name && v["Type"] == r.record_type)
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        bail!("同名同类型记录不唯一，请先整理 DNS 记录");
    }
    let mut params = BTreeMap::from([
        ("RR".into(), name),
        ("Type".into(), r.record_type.clone()),
        ("Value".into(), ip.into()),
        ("TTL".into(), ttl.to_string()),
    ]);
    let action = if let Some(record) = matches.first() {
        if record["Value"] == ip && record["TTL"] == ttl {
            return Ok(false);
        }
        params.insert(
            "RecordId".into(),
            record["RecordId"]
                .as_str()
                .context("Missing record ID")?
                .into(),
        );
        "UpdateDomainRecord"
    } else {
        params.insert("DomainName".into(), r.zone.clone());
        "AddDomainRecord"
    };
    ali_request(client, c, action, params, endpoint).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signing_encoding_matches_aliyun_contract() {
        assert_eq!(percent("a b+~*"), "a%20b%2B~%2A");
    }
}
