//! Runtime settings editable from the web panel.
//!
//! `xbnc.toml` provides the initial values for history and limits. Values
//! saved from the panel are stored in the `settings` table and take
//! precedence; they apply immediately without a restart.

use crate::config::{HistoryConfig, LimitsConfig};
use crate::db::{self, Db};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub history: HistoryConfig,
    pub limits: LimitsConfig,
    #[serde(default)]
    pub general: GeneralSettings,
    #[serde(default)]
    pub alerts: crate::alerts::AlertSettings,
    #[serde(default)]
    pub backups: BackupSettings,
}

/// Automatic backups from the panel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BackupSettings {
    pub enabled: bool,
    pub interval_hours: u32,
    pub keep: u32,
}

impl Default for BackupSettings {
    fn default() -> Self {
        BackupSettings {
            enabled: false,
            interval_hours: 24,
            keep: 7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneralSettings {
    /// Default language for users who have not chosen one.
    pub language: String,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        GeneralSettings {
            language: "en".into(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        self.history.validate()?;
        self.limits.validate()?;
        if crate::i18n::Lang::parse(&self.general.language).is_none() {
            anyhow::bail!("general.language is not a supported language");
        }
        self.alerts.validate()?;
        if !(1..=24 * 30).contains(&self.backups.interval_hours) {
            anyhow::bail!("backups.interval_hours must be between 1 and 720");
        }
        if !(1..=365).contains(&self.backups.keep) {
            anyhow::bail!("backups.keep must be between 1 and 365");
        }
        Ok(())
    }

    /// Language for a user, given their own preference (may be empty).
    pub fn lang_for(&self, user_lang: &str) -> crate::i18n::Lang {
        crate::i18n::Lang::for_user(user_lang, &self.general.language)
    }
}

/// Thread-safe holder of the current settings.
pub struct SettingsStore {
    current: RwLock<Settings>,
    defaults: Settings,
}

const KEY: &str = "runtime";

impl SettingsStore {
    /// Loads defaults from the config file and applies stored overrides.
    pub fn load(
        db: &Db,
        history: HistoryConfig,
        limits: LimitsConfig,
        language: &str,
    ) -> Result<SettingsStore> {
        let defaults = Settings {
            history,
            limits,
            general: GeneralSettings {
                language: language.to_string(),
            },
            alerts: Default::default(),
            backups: Default::default(),
        };
        let stored = db.with(|c| db::load_settings_map(c))?;
        let current = match stored.get(KEY) {
            Some(json) => {
                // Merge stored values over the defaults so new fields added in
                // later versions get their default value.
                let mut base = serde_json::to_value(&defaults)?;
                let over: serde_json::Value =
                    serde_json::from_str(json).context("stored settings are not valid JSON")?;
                merge(&mut base, over);
                match serde_json::from_value::<Settings>(base) {
                    Ok(s) if s.validate().is_ok() => s,
                    _ => {
                        tracing::warn!(
                            "stored settings are invalid; using the configuration file values"
                        );
                        defaults.clone()
                    }
                }
            }
            None => defaults.clone(),
        };
        Ok(SettingsStore {
            current: RwLock::new(current),
            defaults,
        })
    }

    pub fn get(&self) -> Settings {
        self.current.read().expect("settings lock").clone()
    }

    pub fn defaults(&self) -> &Settings {
        &self.defaults
    }

    /// Validates, persists and applies new settings.
    pub async fn save(&self, db: &Db, new: Settings) -> Result<()> {
        new.validate()?;
        let json = serde_json::to_string(&new)?;
        db.call(move |c| db::set_setting(c, KEY, &json)).await?;
        *self.current.write().expect("settings lock") = new;
        Ok(())
    }
}

fn merge(base: &mut serde_json::Value, over: serde_json::Value) {
    match (base, over) {
        (serde_json::Value::Object(b), serde_json::Value::Object(o)) => {
            for (k, v) in o {
                if let Some(slot) = b.get_mut(&k) {
                    merge(slot, v);
                }
            }
        }
        (slot, v) => *slot = v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn overrides_persist_and_merge() {
        let db = Db::open_in_memory().unwrap();
        let store =
            SettingsStore::load(&db, HistoryConfig::default(), LimitsConfig::default(), "en")
                .unwrap();
        assert_eq!(store.get().history.retention_days, 30);
        let mut s = store.get();
        s.history.retention_days = 7;
        store.save(&db, s.clone()).await.unwrap();
        assert_eq!(store.get().history.retention_days, 7);
        let reloaded =
            SettingsStore::load(&db, HistoryConfig::default(), LimitsConfig::default(), "en")
                .unwrap();
        assert_eq!(reloaded.get(), s);
        let mut bad = s.clone();
        bad.limits.client_queue = 1;
        assert!(store.save(&db, bad).await.is_err());
        assert_eq!(store.get().limits.client_queue, s.limits.client_queue);
    }
}
