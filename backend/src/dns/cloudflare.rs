// @created 2026-10-11
// @description 实现 Cloudflare DNS 记录查询与幂等更新。
// @author yunhungo
use super::provider::secret;
use crate::models::{Credential, Ddns};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
async fn response_json(request: reqwest::RequestBuilder) -> Result<Value> {
    let response = request.send().await.context("Cloudflare 连接失败")?;
    let success = response.status().is_success();
    let json: Value = response.json().await.context("Cloudflare 返回无效数据")?;
    if !success || json["success"] != true {
        bail!("Cloudflare 请求失败，请检查 Token 权限和主域名");
    }
    Ok(json["result"].clone())
}
pub async fn update(client: &reqwest::Client, c: &Credential, r: &Ddns, ip: &str) -> Result<bool> {
    update_at(client, c, r, ip, "https://api.cloudflare.com/client/v4").await
}
pub(super) async fn update_at(
    client: &reqwest::Client,
    c: &Credential,
    r: &Ddns,
    ip: &str,
    endpoint: &str,
) -> Result<bool> {
    let token = secret(c, "token")?;
    let zones = response_json(
        client
            .get(format!("{endpoint}/zones"))
            .bearer_auth(token)
            .query(&[("name", r.zone.as_str()), ("status", "active")]),
    )
    .await?;
    let zid = zones[0]["id"].as_str().context("Cloudflare 主域名不存在")?;
    let base = format!("{endpoint}/zones/{zid}/dns_records");
    let records = response_json(client.get(&base).bearer_auth(token).query(&[
        ("name", r.domain.as_str()),
        ("type", r.record_type.as_str()),
    ]))
    .await?;
    let array = records.as_array().context("Cloudflare 记录格式无效")?;
    if array.len() > 1 {
        bail!("同名同类型记录不唯一，请先整理 DNS 记录");
    }
    let ttl = if r.proxied { 1 } else { r.ttl };
    let payload =
        json!({"type":r.record_type,"name":r.domain,"content":ip,"ttl":ttl,"proxied":r.proxied});
    if let Some(record) = array.first() {
        if record["content"] == ip && record["ttl"] == ttl && record["proxied"] == r.proxied {
            return Ok(false);
        }
        response_json(
            client
                .patch(format!(
                    "{base}/{}",
                    record["id"].as_str().context("Missing record ID")?
                ))
                .bearer_auth(token)
                .json(&payload),
        )
        .await?;
    } else {
        response_json(client.post(base).bearer_auth(token).json(&payload)).await?;
    }
    Ok(true)
}
