// @created 2026-10-11
// @description 实现腾讯云 DNSPod TC3 认证与记录更新。
// @author yunhungo
use super::provider::rr;
use super::provider::secret;
use crate::models::{Credential, Ddns};
use anyhow::{bail, Context, Result};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
fn hmac256(key: &[u8], data: &str) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC supports arbitrary keys");
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}
async fn tc_request(
    client: &reqwest::Client,
    c: &Credential,
    action: &str,
    payload: Value,
    endpoint: &str,
) -> Result<Value> {
    let now = chrono::Utc::now();
    let timestamp = now.timestamp();
    let date = now.format("%Y-%m-%d").to_string();
    let body = payload.to_string();
    let content = "application/json; charset=utf-8";
    let canonical=format!("POST\n/\n\ncontent-type:{content}\nhost:dnspod.tencentcloudapi.com\n\ncontent-type;host\n{}",hex::encode(Sha256::digest(body.as_bytes())));
    let scope = format!("{date}/dnspod/tc3_request");
    let string = format!(
        "TC3-HMAC-SHA256\n{timestamp}\n{scope}\n{}",
        hex::encode(Sha256::digest(canonical.as_bytes()))
    );
    let kdate = hmac256(format!("TC3{}", secret(c, "key_secret")?).as_bytes(), &date);
    let kservice = hmac256(&kdate, "dnspod");
    let ksign = hmac256(&kservice, "tc3_request");
    let authorization = format!(
        "TC3-HMAC-SHA256 Credential={}/{scope}, SignedHeaders=content-type;host, Signature={}",
        secret(c, "key_id")?,
        hex::encode(hmac256(&ksign, &string))
    );
    let response = client
        .post(endpoint)
        .header("Content-Type", content)
        .header("Authorization", authorization)
        .header("X-TC-Action", action)
        .header("X-TC-Version", "2021-03-23")
        .header("X-TC-Timestamp", timestamp.to_string())
        .body(body)
        .send()
        .await
        .context("DNSPod 连接失败")?;
    let success = response.status().is_success();
    let value: Value = response.json().await.context("DNSPod 返回无效数据")?;
    if !success {
        bail!("DNSPod HTTP 请求失败");
    }
    Ok(value["Response"].clone())
}
pub async fn update(client: &reqwest::Client, c: &Credential, r: &Ddns, ip: &str) -> Result<bool> {
    update_at(client, c, r, ip, "https://dnspod.tencentcloudapi.com/").await
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
    let result = tc_request(
        client,
        c,
        "DescribeRecordList",
        json!({"Domain":r.zone,"Subdomain":name,"RecordType":r.record_type,"Limit":100}),
        endpoint,
    )
    .await?;
    let code = result["Error"]["Code"].as_str().unwrap_or("");
    if !code.is_empty() && code != "ResourceNotFound.NoDataOfRecord" {
        bail!("DNSPod 请求失败：{code}");
    }
    let records = result["RecordList"].as_array().cloned().unwrap_or_default();
    let records = records
        .iter()
        .filter(|v| v["Name"] == name && v["Type"] == r.record_type)
        .collect::<Vec<_>>();
    if records.len() > 1 {
        bail!("同名同类型记录不唯一，请先整理 DNS 记录");
    }
    let mut payload = json!({"Domain":r.zone,"SubDomain":name,"RecordType":r.record_type,"RecordLine":"默认","Value":ip,"TTL":ttl});
    let action = if let Some(record) = records.first() {
        if record["Value"] == ip && record["TTL"] == ttl {
            return Ok(false);
        }
        payload["RecordId"] = record["RecordId"].clone();
        payload["RecordLine"] = record["Line"].clone();
        "ModifyRecord"
    } else {
        "CreateRecord"
    };
    let result = tc_request(client, c, action, payload, endpoint).await?;
    if let Some(code) = result["Error"]["Code"].as_str() {
        bail!("DNSPod 请求失败：{code}");
    }
    Ok(true)
}
