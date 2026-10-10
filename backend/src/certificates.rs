// @created 2026-10-11
// @description 编排 DNS-01 ACME、导入验证和证书原子切换。
// @author yunhungo
use crate::{
    error::ApiError,
    models::{Certificate, Credential},
    store::private_write,
    AppState,
};
use anyhow::{bail, Context, Result};
use axum::{
    extract::{Path, State},
    Json,
};
use rustls::{pki_types::PrivateKeyDer, sign::CertifiedKey};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{io::Cursor, path::Path as FsPath, sync::Arc, time::Duration};
use x509_parser::extensions::GeneralName;

pub fn covers(pattern: &str, domain: &str) -> bool {
    if pattern == domain {
        return true;
    }
    pattern.strip_prefix("*.").is_some_and(|suffix| {
        domain
            .strip_suffix(&format!(".{suffix}"))
            .is_some_and(|prefix| !prefix.is_empty() && !prefix.contains('.'))
    })
}
pub fn parse_bundle(
    cert: &[u8],
    key: &[u8],
    domains: &[String],
) -> Result<(Arc<CertifiedKey>, Value)> {
    let chain = rustls_pemfile::certs(&mut Cursor::new(cert))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let key: PrivateKeyDer =
        rustls_pemfile::private_key(&mut Cursor::new(key))?.context("私钥 PEM 格式无效")?;
    let (_, x509) =
        x509_parser::parse_x509_certificate(chain.first().context("证书 PEM 格式无效")?.as_ref())
            .map_err(|_| anyhow::anyhow!("证书格式无效"))?;
    let actual = x509
        .subject_alternative_name()?
        .context("证书缺少 SAN 域名")?
        .value
        .general_names
        .iter()
        .filter_map(|name| match name {
            GeneralName::DNSName(name) => Some(name.to_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if domains
        .iter()
        .any(|d| !actual.iter().any(|a| covers(a, d) || a == d))
    {
        bail!("证书 SAN 不覆盖配置域名");
    }
    let expiry = x509.validity().not_after.timestamp();
    if !x509.validity().is_valid() {
        bail!("证书尚未生效或已过期");
    }
    let metadata = json!({"state":"ready","expires_at":expiry,"issuer":x509.issuer().to_string(),"domains":actual,"serial":x509.raw_serial_as_string(),"last_attempt":chrono::Utc::now().timestamp()});
    let signing =
        rustls::crypto::ring::sign::any_supported_type(&key).context("不支持的私钥类型")?;
    let certified = CertifiedKey::new(chain, signing);
    certified.keys_match().context("证书与私钥不匹配")?;
    Ok((Arc::new(certified), metadata))
}
pub async fn load(root: &FsPath, cert: &Certificate) -> Result<crate::proxy::ActiveCertificate> {
    let live = root.join("certificates").join(&cert.id).join("live");
    let pem = tokio::fs::read(live.join("cert.pem")).await?;
    let key = tokio::fs::read(live.join("key.pem")).await?;
    let (key, metadata) = parse_bundle(&pem, &key, &cert.domains)?;
    Ok(crate::proxy::ActiveCertificate {
        key,
        expires_at: metadata["expires_at"]
            .as_i64()
            .context("Invalid certificate expiry")?,
    })
}
pub async fn activate(
    state: &AppState,
    cert: &Certificate,
    pem: &[u8],
    key: &[u8],
) -> Result<Value> {
    let (_, metadata) = parse_bundle(pem, key, &cert.domains)?;
    let root = state.config.data_dir.join("certificates").join(&cert.id);
    let version = uuid::Uuid::new_v4().to_string();
    let dir = root.join("versions").join(&version);
    tokio::fs::create_dir_all(&dir).await?;
    private_write(&dir.join("cert.pem"), pem).await?;
    private_write(&dir.join("key.pem"), key).await?;
    let link = root.join(format!("live-{version}"));
    #[cfg(unix)]
    std::os::unix::fs::symlink(format!("versions/{version}"), &link)?;
    tokio::fs::rename(link, root.join("live")).await?;
    Ok(metadata)
}
pub async fn issue(state: &AppState, cert: &Certificate, _manual: bool) -> Result<Value> {
    if cert.imported {
        bail!("导入证书需重新上传，不能自动签发");
    }
    let credential: Credential = state.store.get("credentials", &cert.credential_id).await?;
    let root = state
        .config
        .data_dir
        .join("certificates")
        .join(&cert.id)
        .join("acme");
    tokio::fs::create_dir_all(&root).await?;
    let mut command = tokio::process::Command::new(&state.config.lego_bin);
    command
        .kill_on_drop(true)
        .arg("--path")
        .arg(&root)
        .arg("--accept-tos")
        .arg("--email")
        .arg(&cert.email)
        .arg("--dns")
        .arg(&credential.provider)
        .arg("--key-type")
        .arg("ec256");
    if cert.staging {
        command
            .arg("--server")
            .arg("https://acme-staging-v02.api.letsencrypt.org/directory");
    }
    for domain in &cert.domains {
        command.arg("--domains").arg(domain);
    }
    for &(env, key) in crate::dns::provider::Provider::parse(&credential.provider)?.acme_env() {
        command.env(env, credential.secrets.get(key).context("缺少 DNS 凭据")?);
    }
    let basename = cert.domains[0].replace('*', "_");
    let pem_path = root.join("certificates").join(format!("{basename}.crt"));
    let key_path = root.join("certificates").join(format!("{basename}.key"));
    if pem_path.exists() {
        command.arg("renew").arg("--days").arg("365");
    } else {
        command.arg("run");
    }
    let output = tokio::time::timeout(Duration::from_secs(900), command.output())
        .await
        .context("证书任务超时，稍后自动重试")?
        .context("无法启动 ACME 客户端")?;
    if !output.status.success() {
        // 上游 stderr 可能包含请求凭据，仅保留经过凭据替换和长度限制的末尾内容。
        let mut error = String::from_utf8_lossy(&output.stderr).into_owned();
        for value in credential.secrets.values() {
            if !value.is_empty() {
                error = error.replace(value, "[redacted]");
            }
        }
        let tail = error
            .lines()
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" ");
        bail!(
            "ACME 签发失败：{}",
            tail.chars().take(350).collect::<String>()
        );
    }
    activate(
        state,
        cert,
        &tokio::fs::read(pem_path).await?,
        &tokio::fs::read(key_path).await?,
    )
    .await
}
#[derive(Deserialize)]
pub struct ImportBundle {
    cert_pem: String,
    key_pem: String,
}
pub async fn import(
    State(state): State<AppState>,
    Path(id): Path<String>,
    crate::error::ApiJson(body): crate::error::ApiJson<ImportBundle>,
) -> Result<Json<Value>, ApiError> {
    let _lock = state.mutations.lock().await;
    if state
        .jobs
        .lock()
        .await
        .contains(&format!("certificates:{id}"))
    {
        return Err(ApiError::bad("证书任务正在执行"));
    }
    let cert: Certificate = state.store.get("certificates", &id).await?;
    if !cert.imported {
        return Err(ApiError::bad("请创建导入类型的证书"));
    }
    let status = activate(
        &state,
        &cert,
        body.cert_pem.as_bytes(),
        body.key_pem.as_bytes(),
    )
    .await?;
    state.store.status("certificates", &id, status).await?;
    crate::proxy::refresh(&state).await?;
    state
        .store
        .event("info", "certificates", "证书已导入")
        .await;
    Ok(Json(json!({"ok":true})))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wildcard_covers_only_one_label() {
        assert!(covers("*.example.com", "a.example.com"));
        assert!(!covers("*.example.com", "a.b.example.com"));
        assert!(!covers("*.example.com", "example.com"));
        assert!(!covers("*.example.com", "evilexample.com"));
    }
}
