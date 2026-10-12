// @created 2026-10-11
// @description 持久化配置、加密 DNS 凭据并维护运行事件。
// @author yunhungo
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::{rngs::OsRng, RngCore};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};
use std::{path::Path, str::FromStr};

#[derive(Clone)]
pub struct Store {
    pub pool: SqlitePool,
    cipher: Aes256Gcm,
}

pub async fn private_write(path: &Path, data: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(path)?;
    std::io::Write::write_all(&mut file, data)?;
    file.sync_all()?;
    Ok(())
}

impl Store {
    pub async fn open(root: &Path) -> Result<Self> {
        tokio::fs::create_dir_all(root).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700)).await?;
        }
        let path = root.join("encryption.key");
        let key = if path.exists() {
            tokio::fs::read(&path).await?
        } else {
            let mut key = vec![0u8; 32];
            OsRng.fill_bytes(&mut key);
            private_write(&path, &key).await?;
            key
        };
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| anyhow::anyhow!("Invalid encryption key"))?;
        let opts = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}",
            root.join("openlucky.sqlite").display()
        ))?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(10));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool, cipher })
    }
    pub fn encrypt(&self, value: &Value) -> Result<String> {
        let mut nonce = [0; 12];
        OsRng.fill_bytes(&mut nonce);
        let data = self
            .cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                serde_json::to_vec(value)?.as_ref(),
            )
            .map_err(|_| anyhow::anyhow!("Encryption failed"))?;
        Ok(STANDARD.encode([nonce.as_slice(), &data].concat()))
    }
    pub fn decrypt(&self, value: &str) -> Result<Value> {
        let bytes = STANDARD.decode(value)?;
        anyhow::ensure!(bytes.len() >= 28, "Invalid encrypted credential");
        let data = self
            .cipher
            .decrypt(Nonce::from_slice(&bytes[..12]), &bytes[12..])
            .map_err(|_| anyhow::anyhow!("Decryption failed"))?;
        Ok(serde_json::from_slice(&data)?)
    }
    pub async fn list(&self, kind: &str) -> Result<Vec<Value>> {
        let rows = sqlx::query(
            "SELECT data,status,revision FROM resources WHERE kind=? ORDER BY rowid DESC",
        )
        .bind(kind)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                let mut data: Value = serde_json::from_str(r.get("data"))?;
                data["status"] = serde_json::from_str(r.get("status"))?;
                data["revision"] = json!(r.get::<i64, _>("revision"));
                if kind == "certificates"
                    && data["status"]["expires_at"]
                        .as_i64()
                        .is_some_and(|expiry| expiry <= chrono::Utc::now().timestamp())
                    && data["status"]["state"] != "running"
                {
                    data["status"]["state"] = json!("error");
                    data["status"]["message"] = json!("证书已过期，请续期或重新导入");
                }
                if kind == "credentials" {
                    data.as_object_mut()
                        .context("Expected object")?
                        .remove("encrypted");
                    data["secrets"] = json!({});
                    data["has_secret"] = json!(true);
                }
                Ok(data)
            })
            .collect()
    }
    pub async fn get<T: DeserializeOwned>(&self, kind: &str, id: &str) -> Result<T> {
        let row = sqlx::query("SELECT data,revision FROM resources WHERE kind=? AND id=?")
            .bind(kind)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .context("资源不存在")?;
        let mut data: Value = serde_json::from_str(row.get("data"))?;
        data["revision"] = json!(row.get::<i64, _>("revision"));
        if kind == "credentials" {
            data["secrets"] =
                self.decrypt(data["encrypted"].as_str().context("Missing credential")?)?;
            data.as_object_mut()
                .context("Invalid object")?
                .remove("encrypted");
        }
        Ok(serde_json::from_value(data)?)
    }
    pub async fn save(&self, kind: &str, id: &str, mut value: Value) -> Result<()> {
        if kind == "credentials" {
            value["encrypted"] = json!(self.encrypt(&value["secrets"])?);
            value
                .as_object_mut()
                .context("Expected object")?
                .remove("secrets");
        }
        sqlx::query("INSERT INTO resources(kind,id,data) VALUES(?,?,?) ON CONFLICT(kind,id) DO UPDATE SET data=excluded.data,revision=revision+1").bind(kind).bind(id).bind(value.to_string()).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn status(&self, kind: &str, id: &str, status: Value) -> Result<()> {
        sqlx::query("UPDATE resources SET status=? WHERE kind=? AND id=?")
            .bind(status.to_string())
            .bind(kind)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn event(&self, level: &str, module: &str, message: &str) {
        let result = sqlx::query("INSERT INTO events(time,level,module,message) VALUES(?,?,?,?)")
            .bind(chrono::Utc::now().timestamp())
            .bind(level)
            .bind(module)
            .bind(crate::telemetry::Telemetry::bounded(message, 2048))
            .execute(&self.pool)
            .await;
        if result.is_err() {
            tracing::warn!("Could not persist event");
        }
        let _ = sqlx::query("DELETE FROM events WHERE id NOT IN (SELECT id FROM events ORDER BY id DESC LIMIT 2000)").execute(&self.pool).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn secrets_are_encrypted_and_redacted() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).await.unwrap();
        store.save("credentials", "a", json!({"id":"a","name":"test","provider":"cloudflare","secrets":{"token":"sensitive"}})).await.unwrap();
        let raw: String = sqlx::query_scalar("SELECT data FROM resources")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert!(!raw.contains("sensitive"));
        assert_eq!(
            store.get::<Value>("credentials", "a").await.unwrap()["secrets"]["token"],
            "sensitive"
        );
        assert!(
            !serde_json::to_string(&store.list("credentials").await.unwrap())
                .unwrap()
                .contains("sensitive")
        );
        let encrypted = store.encrypt(&json!({"secret":"x"})).unwrap();
        assert!(store.decrypt(&(encrypted + "invalid")).is_err());
    }
}
