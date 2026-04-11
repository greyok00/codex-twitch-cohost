use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
};

use crate::error::{AppError, AppResult};

const SERVICE_NAME: &str = "twitch-cohost-bot";

#[derive(Clone, Default)]
pub struct SecretStore;

impl SecretStore {
    pub fn new() -> Self {
        Self
    }

    pub fn set_twitch_token(&self, channel: &str, token: &str) -> AppResult<()> {
        self.set_secret(&format!("twitch:{channel}"), token)
    }

    pub fn get_twitch_token(&self, channel: &str) -> AppResult<Option<String>> {
        self.get_secret(&format!("twitch:{channel}"))
    }

    pub fn set_twitch_refresh_token(&self, channel: &str, token: &str) -> AppResult<()> {
        self.set_secret(&format!("twitch-refresh:{channel}"), token)
    }

    pub fn get_twitch_refresh_token(&self, channel: &str) -> AppResult<Option<String>> {
        self.get_secret(&format!("twitch-refresh:{channel}"))
    }

    pub fn set_twitch_client_secret(&self, client_id: &str, secret: &str) -> AppResult<()> {
        self.set_secret(&format!("twitch-client-secret:{client_id}"), secret)
    }

    pub fn get_twitch_client_secret(&self, client_id: &str) -> AppResult<Option<String>> {
        self.get_secret(&format!("twitch-client-secret:{client_id}"))
    }

    pub fn set_provider_key(&self, provider_name: &str, key: &str) -> AppResult<()> {
        self.set_secret(&format!("provider:{provider_name}"), key)
    }

    pub fn get_provider_key(&self, provider_name: &str) -> AppResult<Option<String>> {
        self.get_secret(&format!("provider:{provider_name}"))
    }

    pub fn clear_all_twitch_sessions(&self) -> AppResult<()> {
        let path = Self::secrets_path();
        if path.exists() {
            let raw = fs::read_to_string(&path).map_err(|e| {
                AppError::SecretStore(format!("failed reading {}: {e}", path.display()))
            })?;
            let mut map: HashMap<String, String> = serde_json::from_str(&raw).unwrap_or_default();
            let keys = map
                .keys()
                .filter(|k| k.starts_with("twitch:") || k.starts_with("twitch-refresh:"))
                .cloned()
                .collect::<Vec<_>>();
            for key in keys {
                map.remove(&key);
                let _ = self.delete_secret(&key);
            }
            let rendered = serde_json::to_string_pretty(&map).map_err(|e| {
                AppError::SecretStore(format!("failed encoding local secrets: {e}"))
            })?;
            fs::write(&path, rendered).map_err(|e| {
                AppError::SecretStore(format!("failed writing {}: {e}", path.display()))
            })?;
        }
        Ok(())
    }

    fn set_secret(&self, account: &str, secret: &str) -> AppResult<()> {
        if let Ok(entry) = keyring::Entry::new(SERVICE_NAME, account) {
            let _ = entry.set_password(secret);
        }
        self.write_local_secret(account, secret)
    }

    fn get_secret(&self, account: &str) -> AppResult<Option<String>> {
        if let Ok(entry) = keyring::Entry::new(SERVICE_NAME, account) {
            match entry.get_password() {
                Ok(secret) => return Ok(Some(secret)),
                Err(keyring::Error::NoEntry) => {}
                Err(_) => {}
            }
        }
        self.read_local_secret(account)
    }

    fn secrets_path() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            if let Ok(appdata) = std::env::var("APPDATA") {
                return PathBuf::from(appdata).join("twitch-cohost-bot").join("secrets.json");
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
                return PathBuf::from(xdg).join("twitch-cohost-bot").join("secrets.json");
            }
            if let Ok(home) = std::env::var("HOME") {
                return PathBuf::from(home)
                    .join(".config")
                    .join("twitch-cohost-bot")
                    .join("secrets.json");
            }
        }

        PathBuf::from("/tmp/twitch-cohost-bot-secrets.json")
    }

    fn read_local_secret(&self, account: &str) -> AppResult<Option<String>> {
        let path = Self::secrets_path();
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| AppError::SecretStore(format!("failed reading {}: {e}", path.display())))?;
        let map: HashMap<String, String> = serde_json::from_str(&raw)
            .map_err(|e| AppError::SecretStore(format!("failed parsing {}: {e}", path.display())))?;
        Ok(map.get(account).cloned())
    }

    fn write_local_secret(&self, account: &str, secret: &str) -> AppResult<()> {
        let path = Self::secrets_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::SecretStore(format!("failed creating {}: {e}", parent.display()))
            })?;
        }

        let mut map = if path.exists() {
            let raw = fs::read_to_string(&path).map_err(|e| {
                AppError::SecretStore(format!("failed reading {}: {e}", path.display()))
            })?;
            serde_json::from_str::<HashMap<String, String>>(&raw).unwrap_or_default()
        } else {
            HashMap::new()
        };
        map.insert(account.to_string(), secret.to_string());
        let rendered = serde_json::to_string_pretty(&map)
            .map_err(|e| AppError::SecretStore(format!("failed encoding local secrets: {e}")))?;
        fs::write(&path, rendered)
            .map_err(|e| AppError::SecretStore(format!("failed writing {}: {e}", path.display())))
    }

    fn delete_secret(&self, account: &str) -> AppResult<()> {
        if let Ok(entry) = keyring::Entry::new(SERVICE_NAME, account) {
            match entry.delete_credential() {
                Ok(()) => {}
                Err(keyring::Error::NoEntry) => {}
                Err(err) => {
                    return Err(AppError::SecretStore(format!(
                        "failed deleting keyring credential for {account}: {err}"
                    )));
                }
            }
        }
        Ok(())
    }
}
