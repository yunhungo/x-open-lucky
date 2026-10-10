// @created 2026-10-11
// @description 使用 Argon2id 和 AES-GCM 导出及事务恢复可迁移配置备份。
// @author yunhungo
use crate::{
    error::ApiError,
    models::{self, Certificate},
    AppState,
};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use axum::{extract::State, Json};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

#[derive(Serialize, Deserialize)]
struct Bundle {
    version: u8,
    resources: BTreeMap<String, Vec<Value>>,
    certificates: BTreeMap<String, Pem>,
}
#[derive(Serialize, Deserialize)]
struct Pem {
    cert: String,
    key: String,
}
#[derive(Serialize, Deserialize)]
struct Envelope {
    format: String,
    salt: String,
    nonce: String,
    data: String,
}
#[derive(Deserialize)]
pub struct BackupRequest {
    password: String,
    #[serde(default)]
    backup: String,
}
async fn key(password: String, salt: Vec<u8>) -> Result<[u8; 32]> {
    tokio::task::spawn_blocking(move || {
        let mut key = [0; 32];
        argon2::Argon2::default()
            .hash_password_into(password.as_bytes(), &salt, &mut key)
            .map_err(|_| anyhow::anyhow!("备份密码处理失败"))?;
        Ok(key)
    })
    .await?
}
pub async fn export(
    State(state): State<AppState>,
    crate::error::ApiJson(body): crate::error::ApiJson<BackupRequest>,
) -> Result<Json<Value>, ApiError> {
    crate::auth::valid_password(&body.password)?;
    let _lock = state.mutations.lock().await;
    if !state.jobs.lock().await.is_empty() {
        return Err(ApiError::bad("请等待任务结束后备份"));
    }
    let mut bundle = Bundle {
        version: 1,
        resources: BTreeMap::new(),
        certificates: BTreeMap::new(),
    };
    for kind in ["credentials", "ddns", "web", "certificates"] {
        let mut items = Vec::new();
        for item in state.store.list(kind).await? {
            let id = item["id"].as_str().context("Invalid ID")?;
            let data = state.store.get::<Value>(kind, id).await?;
            if kind == "certificates" {
                let path = state
                    .config
                    .data_dir
                    .join("certificates")
                    .join(id)
                    .join("live");
                if path.exists() {
                    bundle.certificates.insert(
                        id.into(),
                        Pem {
                            cert: tokio::fs::read_to_string(path.join("cert.pem"))
                                .await
                                .map_err(|_| ApiError::bad("无法读取证书"))?,
                            key: tokio::fs::read_to_string(path.join("key.pem"))
                                .await
                                .map_err(|_| ApiError::bad("无法读取私钥"))?,
                        },
                    );
                }
            }
            items.push(data);
        }
        bundle.resources.insert(kind.into(), items);
    }
    let mut salt = [0; 16];
    let mut nonce = [0; 12];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let derived = key(body.password, salt.to_vec()).await?;
    let cipher = Aes256Gcm::new_from_slice(&derived).map_err(|_| ApiError::bad("备份加密失败"))?;
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            serde_json::to_vec(&bundle)
                .map_err(|_| ApiError::bad("备份序列化失败"))?
                .as_ref(),
        )
        .map_err(|_| ApiError::bad("备份加密失败"))?;
    let envelope = Envelope {
        format: "openlucky-backup-v1".into(),
        salt: STANDARD.encode(salt),
        nonce: STANDARD.encode(nonce),
        data: STANDARD.encode(encrypted),
    };
    state
        .store
        .event("info", "system", "加密配置备份已导出")
        .await;
    Ok(Json(
        json!({"backup":serde_json::to_string_pretty(&envelope).map_err(|_|ApiError::bad("备份导出失败"))?}),
    ))
}
pub async fn restore(
    State(state): State<AppState>,
    crate::error::ApiJson(body): crate::error::ApiJson<BackupRequest>,
) -> Result<Json<Value>, ApiError> {
    crate::auth::valid_password(&body.password)?;
    let envelope: Envelope =
        serde_json::from_str(&body.backup).map_err(|_| ApiError::bad("备份格式无效"))?;
    if envelope.format != "openlucky-backup-v1" {
        return Err(ApiError::bad("不支持的备份版本"));
    }
    let salt = STANDARD
        .decode(envelope.salt)
        .map_err(|_| ApiError::bad("备份格式无效"))?;
    let nonce = STANDARD
        .decode(envelope.nonce)
        .map_err(|_| ApiError::bad("备份格式无效"))?;
    if salt.len() != 16 || nonce.len() != 12 {
        return Err(ApiError::bad("备份格式无效"));
    }
    let key = key(body.password, salt).await?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| ApiError::bad("恢复失败"))?;
    let bytes = STANDARD
        .decode(envelope.data)
        .map_err(|_| ApiError::bad("备份格式无效"))?;
    let decrypted = cipher
        .decrypt(Nonce::from_slice(&nonce), bytes.as_ref())
        .map_err(|_| ApiError::bad("备份密码错误或文件损坏"))?;
    let bundle: Bundle =
        serde_json::from_slice(&decrypted).map_err(|_| ApiError::bad("备份内容无效"))?;
    if bundle.version != 1
        || bundle.resources.values().map(Vec::len).sum::<usize>() > 1000
        || bundle.resources.len() != 4
        || ["credentials", "ddns", "web", "certificates"]
            .iter()
            .any(|k| !bundle.resources.contains_key(*k))
    {
        return Err(ApiError::bad("备份版本或资源数量无效"));
    }
    let _lock = state.mutations.lock().await;
    if !state.jobs.lock().await.is_empty() {
        return Err(ApiError::bad("请等待所有任务结束后恢复"));
    }
    let mut ids = BTreeMap::new();
    let mut validated = BTreeMap::new();
    for (kind, items) in bundle.resources {
        let mut output = Vec::new();
        for item in items {
            let old = item["id"].as_str().context("备份资源缺少 ID")?.to_owned();
            let mut value = models::validate(&kind, item)?;
            let id = uuid::Uuid::new_v4().to_string();
            if ids.insert(old.clone(), id.clone()).is_some() {
                return Err(ApiError::bad("备份资源 ID 重复"));
            }
            value["id"] = json!(id);
            output.push((old, value));
        }
        validated.insert(kind, output);
    }
    for items in validated.values_mut() {
        for (_, item) in items {
            for field in ["credential_id", "certificate_id"] {
                if let Some(old) = item[field].as_str().filter(|s| !s.is_empty()) {
                    item[field] = json!(ids.get(old).context("备份资源关联缺失")?);
                }
            }
        }
    }
    let credentials = validated
        .get("credentials")
        .context("缺少凭据资源")?
        .iter()
        .map(|(_, v)| (v["id"].as_str().unwrap_or_default(), v))
        .collect::<BTreeMap<_, _>>();
    for credential in credentials.values() {
        for field in crate::dns::provider::Provider::parse(
            credential["provider"].as_str().unwrap_or_default(),
        )?
        .fields()
        {
            if credential["secrets"][*field]
                .as_str()
                .is_none_or(|v| v.is_empty() || v.len() > 2048 || v.contains('\n'))
            {
                return Err(ApiError::bad("备份凭据字段无效"));
            }
        }
    }
    let certs = validated
        .get("certificates")
        .context("缺少证书资源")?
        .iter()
        .map(|(_, v)| (v["id"].as_str().unwrap_or_default(), v))
        .collect::<BTreeMap<_, _>>();
    for cert in certs.values() {
        if cert["imported"] != true
            && !credentials.contains_key(cert["credential_id"].as_str().unwrap_or_default())
        {
            return Err(ApiError::bad("证书的 DNS 凭据不存在"));
        }
    }
    let mut domains = HashSet::new();
    for (_, rule) in validated.get("ddns").into_iter().flatten() {
        let credential = credentials
            .get(rule["credential_id"].as_str().unwrap_or_default())
            .context("DDNS 凭据不存在")?;
        if !domains.insert(format!("{}:{}", rule["domain"], rule["record_type"])) {
            return Err(ApiError::bad("备份包含重复 DNS 更新任务"));
        }
        if credential["provider"] != "cloudflare"
            && (rule["proxied"] == true || rule["ttl"].as_u64().unwrap_or(0) < 600)
        {
            return Err(ApiError::bad("备份中的 DNS 参数与服务商不兼容"));
        }
    }
    domains.clear();
    for (_, rule) in validated.get("web").into_iter().flatten() {
        if !domains.insert(rule["domain"].as_str().unwrap_or_default().to_owned()) {
            return Err(ApiError::bad("备份包含重复 Web 域名"));
        }
        if let Some(cid) = rule["certificate_id"].as_str().filter(|v| !v.is_empty()) {
            let cert = certs.get(cid).context("Web 证书不存在")?;
            if !cert["domains"]
                .as_array()
                .context("Invalid domains")?
                .iter()
                .any(|d| {
                    crate::certificates::covers(
                        d.as_str().unwrap_or_default(),
                        rule["domain"].as_str().unwrap_or_default(),
                    )
                })
            {
                return Err(ApiError::bad("备份中的证书不覆盖 Web 域名"));
            }
        }
    }
    // 先校验所有 PEM，随后写入全新目录；失败不会覆盖正在使用的证书。
    for (old, value) in validated.get("certificates").into_iter().flatten() {
        if let Some(pem) = bundle.certificates.get(old) {
            let cert: Certificate =
                serde_json::from_value(value.clone()).map_err(|_| ApiError::bad("证书配置无效"))?;
            crate::certificates::parse_bundle(
                pem.cert.as_bytes(),
                pem.key.as_bytes(),
                &cert.domains,
            )?;
        }
    }
    let mut statuses = BTreeMap::new();
    for (old, value) in validated.get("certificates").into_iter().flatten() {
        if let Some(pem) = bundle.certificates.get(old) {
            let cert: Certificate =
                serde_json::from_value(value.clone()).map_err(|_| ApiError::bad("证书配置无效"))?;
            statuses.insert(
                cert.id.clone(),
                crate::certificates::activate(
                    &state,
                    &cert,
                    pem.cert.as_bytes(),
                    pem.key.as_bytes(),
                )
                .await?,
            );
        }
    }
    let mut tx = state.store.pool.begin().await?;
    sqlx::query("DELETE FROM resources")
        .execute(&mut *tx)
        .await?;
    for (kind, items) in validated {
        for (_, mut value) in items {
            if kind == "credentials" {
                value["encrypted"] = json!(state.store.encrypt(&value["secrets"])?);
                value
                    .as_object_mut()
                    .context("Invalid object")?
                    .remove("secrets");
            }
            let id = value["id"].as_str().context("Invalid ID")?;
            let status = statuses.get(id).cloned().unwrap_or(json!({"state":"idle"}));
            sqlx::query("INSERT INTO resources(kind,id,data,status) VALUES(?,?,?,?)")
                .bind(&kind)
                .bind(id)
                .bind(value.to_string())
                .bind(status.to_string())
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    crate::proxy::refresh(&state).await?;
    state
        .store
        .event("info", "system", "配置已从加密备份恢复")
        .await;
    Ok(Json(json!({"ok":true})))
}
