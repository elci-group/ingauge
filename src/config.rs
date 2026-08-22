// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use crate::providers::ProviderError;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

mod validation;

pub const CONFIG_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    #[serde(default)]
    pub general: General,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderConfig>,
    #[serde(default)]
    pub forecast: ForecastConfig,
    #[serde(default)]
    pub admission: AdmissionConfig,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct General {
    #[serde(default = "default_poll")]
    pub poll_interval: String,
    #[serde(default = "default_retention")]
    pub history_retention: String,
    pub database: Option<PathBuf>,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout: String,
    #[serde(default = "default_request_timeout")]
    pub request_timeout: String,
    #[serde(default = "default_response_bytes")]
    pub max_response_bytes: usize,
    #[serde(default = "default_records")]
    pub max_records: usize,
    #[serde(default = "default_attempts")]
    pub max_attempts: u8,
}

impl Default for General {
    fn default() -> Self {
        Self {
            poll_interval: default_poll(),
            history_retention: default_retention(),
            database: None,
            connect_timeout: default_connect_timeout(),
            request_timeout: default_request_timeout(),
            max_response_bytes: default_response_bytes(),
            max_records: default_records(),
            max_attempts: default_attempts(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub enabled: Option<bool>,
    pub endpoint: Option<String>,
    pub api_key_env: Option<String>,
    #[serde(default = "default_usage_path")]
    pub usage_path: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            enabled: None,
            endpoint: None,
            api_key_env: None,
            usage_path: default_usage_path(),
        }
    }
}

fn default_usage_path() -> String {
    "/usage".into()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForecastConfig {
    #[serde(default = "default_samples")]
    pub minimum_samples: usize,
    #[serde(default = "default_window")]
    pub window: String,
    #[serde(default = "default_moderate")]
    pub moderate_threshold: f64,
    #[serde(default = "default_constrained")]
    pub constrained_threshold: f64,
    #[serde(default = "default_critical")]
    pub critical_threshold: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionConfig {
    #[serde(default = "default_admission_enabled")]
    pub enabled: bool,
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    #[serde(default = "default_default_delay_ms")]
    pub default_delay_ms: u64,
    #[serde(default = "default_admit_when_unknown")]
    pub admit_when_unknown: bool,
}

impl Default for AdmissionConfig {
    fn default() -> Self {
        Self {
            enabled: default_admission_enabled(),
            listen_addr: default_listen_addr(),
            max_concurrent: default_max_concurrent(),
            default_delay_ms: default_default_delay_ms(),
            admit_when_unknown: default_admit_when_unknown(),
        }
    }
}

impl Default for ForecastConfig {
    fn default() -> Self {
        Self {
            minimum_samples: default_samples(),
            window: default_window(),
            moderate_threshold: default_moderate(),
            constrained_threshold: default_constrained(),
            critical_threshold: default_critical(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            general: General::default(),
            providers: BTreeMap::new(),
            forecast: ForecastConfig::default(),
            admission: AdmissionConfig::default(),
        }
    }
}

fn default_schema_version() -> u16 {
    CONFIG_SCHEMA_VERSION
}
fn default_poll() -> String {
    "60s".into()
}
fn default_retention() -> String {
    "30d".into()
}
fn default_connect_timeout() -> String {
    "3s".into()
}
fn default_request_timeout() -> String {
    "10s".into()
}
fn default_window() -> String {
    "24h".into()
}
fn default_response_bytes() -> usize {
    1_048_576
}
fn default_records() -> usize {
    10_000
}
fn default_attempts() -> u8 {
    3
}
fn default_samples() -> usize {
    5
}
fn default_moderate() -> f64 {
    0.7
}
fn default_constrained() -> f64 {
    0.3
}
fn default_critical() -> f64 {
    0.1
}
fn default_admission_enabled() -> bool {
    true
}
fn default_listen_addr() -> String {
    "127.0.0.1:8080".into()
}
fn default_max_concurrent() -> usize {
    4
}
fn default_default_delay_ms() -> u64 {
    1_000
}
fn default_admit_when_unknown() -> bool {
    true
}

pub fn parse_duration(value: &str) -> Result<Duration, ProviderError> {
    let split = value.find(|c: char| !c.is_ascii_digit()).ok_or_else(|| {
        ProviderError::Configuration(format!("duration '{value}' requires a unit"))
    })?;
    let amount: u64 = value[..split].parse().map_err(|error| {
        tracing::warn!(event = "duration_parse_failed", value, error = %error, "invalid duration");
        ProviderError::Configuration(format!("invalid duration '{value}': {error}"))
    })?;
    if amount == 0 {
        tracing::warn!(event = "duration_out_of_range", value, "duration rejected");
        return Err(ProviderError::Configuration(
            "durations must be positive".into(),
        ));
    }
    let seconds = match &value[split..] {
        "s" => Some(amount),
        "m" => amount.checked_mul(60),
        "h" => amount.checked_mul(3_600),
        "d" => amount.checked_mul(86_400),
        _ => None,
    }
    .ok_or_else(|| ProviderError::Configuration(format!("invalid duration '{value}'")))?;
    Ok(Duration::from_secs(seconds))
}

impl Config {
    pub fn resolve_path(explicit: Option<&Path>) -> Option<PathBuf> {
        if let Some(path) = explicit {
            return Some(path.to_path_buf());
        }
        if let Some(path) = std::env::var_os("INGAUGE_CONFIG") {
            return Some(PathBuf::from(path));
        }
        std::env::var_os("XDG_CONFIG_HOME")
            .map(|p| PathBuf::from(p).join("ingauge/ingauge.toml"))
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|p| PathBuf::from(p).join(".config/ingauge/ingauge.toml"))
            })
            .filter(|path| path.exists())
    }

    pub fn load(path: Option<&Path>) -> Result<Self, ProviderError> {
        let Some(path) = Self::resolve_path(path) else {
            return Ok(Self::default());
        };
        let text = fs::read_to_string(&path)
            .map_err(|e| ProviderError::Configuration(format!("{}: {e}", path.display())))?;
        toml::from_str(&text).map_err(|e| ProviderError::Configuration(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_parsing_is_bounded_and_typed() {
        assert!(matches!(parse_duration("5m"), Ok(value) if value.as_secs() == 300));
        assert!(parse_duration("0s").is_err());
        assert!(parse_duration("12").is_err());
    }

    #[test]
    fn defaults_validate() {
        assert!(Config::default().validate().is_ok());
    }

    #[test]
    fn rejects_remote_plaintext_endpoint() {
        let mut config = Config::default();
        config.providers.insert(
            "harness".into(),
            ProviderConfig {
                enabled: Some(true),
                endpoint: Some("http://example.com".into()),
                api_key_env: None,
                ..ProviderConfig::default()
            },
        );
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_limits_and_thresholds_but_accepts_custom_targets() {
        let mut config = Config::default();
        config.general.max_attempts = 0;
        assert!(config.validate().is_err());
        config.general.max_attempts = 1;
        config.forecast.minimum_samples = 1;
        assert!(config.validate().is_err());
        config.forecast.minimum_samples = 2;
        config.forecast.critical_threshold = 0.9;
        assert!(config.validate().is_err());
        config.forecast = ForecastConfig::default();
        config
            .providers
            .insert("unknown".into(), ProviderConfig::default());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn explicit_missing_configuration_is_an_error() {
        let path = Path::new("/definitely/missing/ingauge.toml");
        assert_eq!(Config::resolve_path(Some(path)), Some(path.to_path_buf()));
        assert!(Config::load(Some(path)).is_err());
    }

    #[test]
    fn duration_parser_accepts_every_supported_unit() {
        assert!(matches!(parse_duration("2s"), Ok(value) if value.as_secs() == 2));
        assert!(matches!(parse_duration("2m"), Ok(value) if value.as_secs() == 120));
        assert!(matches!(parse_duration("2h"), Ok(value) if value.as_secs() == 7_200));
        assert!(matches!(parse_duration("2d"), Ok(value) if value.as_secs() == 172_800));
        assert!(parse_duration("2weeks").is_err());
    }

    #[test]
    fn rejects_schema_response_record_and_credential_bounds() {
        let mut config = Config {
            schema_version: 99,
            ..Config::default()
        };
        assert!(config.validate().is_err());

        config.schema_version = CONFIG_SCHEMA_VERSION;
        config.general.max_response_bytes = 0;
        assert!(config.validate().is_err());
        config.general.max_response_bytes = default_response_bytes();
        config.general.max_records = 0;
        assert!(config.validate().is_err());

        config.general.max_records = default_records();
        config.providers.insert(
            "harness".into(),
            ProviderConfig {
                enabled: Some(true),
                endpoint: Some("https://example.com".into()),
                api_key_env: Some("lower-case".into()),
                ..ProviderConfig::default()
            },
        );
        assert!(config.validate().is_err());
    }
}
