use crate::providers::ProviderError;
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: General,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderConfig>,
    #[serde(default)]
    pub forecast: ForecastConfig,
}
#[derive(Clone, Debug, Deserialize)]
pub struct General {
    #[serde(default = "default_poll")]
    pub poll_interval: String,
    #[serde(default = "default_retention")]
    pub history_retention: String,
    pub database: Option<String>,
}
fn default_poll() -> String {
    "60s".into()
}
fn default_retention() -> String {
    "30d".into()
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProviderConfig {
    pub enabled: Option<bool>,
    pub endpoint: Option<String>,
    pub api_key_env: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ForecastConfig {
    #[serde(default = "default_samples")]
    pub minimum_samples: usize,
}
fn default_samples() -> usize {
    5
}
impl Default for Config {
    fn default() -> Self {
        Self {
            general: General {
                poll_interval: default_poll(),
                history_retention: default_retention(),
                database: None,
            },
            providers: BTreeMap::new(),
            forecast: ForecastConfig {
                minimum_samples: default_samples(),
            },
        }
    }
}
impl Config {
    pub fn load(path: Option<&Path>) -> Result<Self, ProviderError> {
        match path {
            Some(p) => Ok(toml::from_str(
                &fs::read_to_string(p).map_err(|e| ProviderError::Configuration(e.to_string()))?,
            )
            .map_err(|e| ProviderError::Configuration(e.to_string()))?),
            None => Ok(Self::default()),
        }
    }
    pub fn validate(&self) -> Result<(), ProviderError> {
        for (id, p) in &self.providers {
            if p.enabled.unwrap_or(true) && id == "ollama" {
                if let Some(e) = &p.endpoint {
                    if !e.starts_with("http://127.0.0.1") && !e.starts_with("http://localhost") {
                        return Err(ProviderError::Configuration(
                            "ollama HTTP endpoint must be localhost".into(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
