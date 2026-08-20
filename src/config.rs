use crate::providers::ProviderError;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

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

    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.schema_version != CONFIG_SCHEMA_VERSION {
            tracing::warn!(
                event = "configuration_rejected",
                field = "schema_version",
                actual = self.schema_version,
                "unsupported configuration schema"
            );
            return Err(ProviderError::Configuration(format!(
                "unsupported schema_version {}; expected {CONFIG_SCHEMA_VERSION}",
                self.schema_version
            )));
        }
        let poll = parse_duration(&self.general.poll_interval)?;
        if poll < Duration::from_secs(1) || poll > Duration::from_secs(86_400) {
            tracing::warn!(
                event = "configuration_rejected",
                field = "poll_interval",
                "configuration value out of range"
            );
            return Err(ProviderError::Configuration(
                "poll_interval must be between 1s and 1d".into(),
            ));
        }
        parse_duration(&self.general.history_retention)?;
        parse_duration(&self.general.connect_timeout)?;
        parse_duration(&self.general.request_timeout)?;
        parse_duration(&self.forecast.window)?;
        if self.general.max_response_bytes == 0
            || self.general.max_response_bytes > 64 * 1024 * 1024
        {
            tracing::warn!(
                event = "configuration_rejected",
                field = "max_response_bytes",
                "configuration value out of range"
            );
            return Err(ProviderError::Configuration(
                "max_response_bytes must be between 1 and 67108864".into(),
            ));
        }
        if self.general.max_records == 0 || self.general.max_records > 1_000_000 {
            tracing::warn!(
                event = "configuration_rejected",
                field = "max_records",
                "configuration value out of range"
            );
            return Err(ProviderError::Configuration(
                "max_records must be between 1 and 1000000".into(),
            ));
        }
        if !(1..=10).contains(&self.general.max_attempts) {
            tracing::warn!(
                event = "configuration_rejected",
                field = "max_attempts",
                "configuration value out of range"
            );
            return Err(ProviderError::Configuration(
                "max_attempts must be between 1 and 10".into(),
            ));
        }
        if self.forecast.minimum_samples < 2 {
            tracing::warn!(
                event = "configuration_rejected",
                field = "minimum_samples",
                "configuration value out of range"
            );
            return Err(ProviderError::Configuration(
                "minimum_samples must be at least 2".into(),
            ));
        }
        let (critical, constrained, moderate) = (
            self.forecast.critical_threshold,
            self.forecast.constrained_threshold,
            self.forecast.moderate_threshold,
        );
        if !((0.0..critical).contains(&0.0)
            && critical < constrained
            && constrained < moderate
            && moderate < 1.0)
        {
            tracing::warn!(
                event = "configuration_rejected",
                field = "forecast_thresholds",
                critical,
                constrained,
                moderate,
                "configuration values invalid"
            );
            tracing::warn!(
                event = "configuration_error_returned",
                field = "forecast_thresholds",
                "validation failed"
            );
            return Err(ProviderError::Configuration(
                "thresholds must satisfy 0 < critical < constrained < moderate < 1".into(),
            ));
        }
        for (id, provider) in &self.providers {
            if !provider.enabled.unwrap_or(true) {
                continue;
            }
            crate::model::ProviderId::new(id.clone()).map_err(ProviderError::Configuration)?;
            let endpoint = provider
                .endpoint
                .as_deref()
                .unwrap_or("http://127.0.0.1:3000");
            let url = reqwest::Url::parse(endpoint).map_err(|e| {
                ProviderError::Configuration(format!("invalid endpoint for {id}: {e}"))
            })?;
            let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
            if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
                tracing::warn!(
                    event = "configuration_rejected",
                    field = "endpoint",
                    provider = id,
                    scheme = url.scheme(),
                    "insecure provider endpoint"
                );
                tracing::warn!(
                    event = "configuration_error_returned",
                    field = "endpoint",
                    provider = id,
                    "validation failed"
                );
                return Err(ProviderError::Configuration(format!(
                    "{id} endpoint must use HTTPS unless it is loopback"
                )));
            }
            if let Some(name) = &provider.api_key_env {
                if name.is_empty()
                    || !name
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                {
                    tracing::warn!(
                        event = "configuration_rejected",
                        field = "api_key_env",
                        provider = id,
                        "invalid credential reference"
                    );
                    return Err(ProviderError::Configuration(format!(
                        "invalid api_key_env for {id}"
                    )));
                }
            }
            if !provider.usage_path.starts_with('/')
                || provider.usage_path.contains('?')
                || provider.usage_path.contains('#')
            {
                tracing::warn!(
                    event = "configuration_rejected",
                    field = "usage_path",
                    provider = id,
                    "invalid canonical usage path"
                );
                return Err(ProviderError::Configuration(format!(
                    "invalid usage_path for {id}"
                )));
            }
        }
        Ok(())
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
