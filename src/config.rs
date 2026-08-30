// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use crate::providers::ProviderError;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

mod validation;

pub const CONFIG_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    #[serde(default)]
    pub instruments: InstrumentConfig,
}

/// Calibration for the telemetry-independent instrument layer. The current
/// terminal renderer uses fixed mode; clients may use adaptive or
/// provider-specific calibration without changing telemetry semantics.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstrumentConfig {
    #[serde(default)]
    pub rpm: PerformanceGaugeConfig,
    #[serde(default)]
    pub tpm: PerformanceGaugeConfig,
    #[serde(default)]
    pub rpd: ResourceGaugeConfig,
    #[serde(default = "default_provider_cycle_seconds")]
    pub provider_cycle_seconds: u64,
    #[serde(default = "default_dashboard_sample_seconds")]
    pub dashboard_sample_seconds: u64,
    #[serde(default)]
    pub network: NetworkMonitorConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkMonitorConfig {
    #[serde(default = "default_network_enabled")]
    pub enabled: bool,
    #[serde(default = "default_network_sample_millis")]
    pub sample_interval_ms: u64,
    #[serde(default = "default_network_bytes_per_token")]
    pub bytes_per_token: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PerformanceGaugeConfig {
    #[serde(default)]
    pub min: f64,
    #[serde(default = "default_rpm_max")]
    pub max: f64,
    #[serde(default = "default_rpm_redline")]
    pub redline: f64,
    #[serde(default = "default_scale_mode")]
    pub scale_mode: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceGaugeConfig {
    #[serde(default = "default_rpd_limit")]
    pub daily_limit: f64,
    #[serde(default = "default_rpd_warning")]
    pub warning: f64,
    #[serde(default = "default_rpd_critical")]
    pub critical: f64,
}

impl Default for InstrumentConfig {
    fn default() -> Self {
        Self {
            rpm: PerformanceGaugeConfig::default(),
            tpm: PerformanceGaugeConfig {
                max: 100_000.0,
                redline: 85_000.0,
                ..PerformanceGaugeConfig::default()
            },
            rpd: ResourceGaugeConfig::default(),
            provider_cycle_seconds: default_provider_cycle_seconds(),
            dashboard_sample_seconds: default_dashboard_sample_seconds(),
            network: NetworkMonitorConfig::default(),
        }
    }
}
impl Default for NetworkMonitorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sample_interval_ms: default_network_sample_millis(),
            bytes_per_token: default_network_bytes_per_token(),
        }
    }
}
impl Default for PerformanceGaugeConfig {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: default_rpm_max(),
            redline: default_rpm_redline(),
            scale_mode: default_scale_mode(),
        }
    }
}
impl Default for ResourceGaugeConfig {
    fn default() -> Self {
        Self {
            daily_limit: default_rpd_limit(),
            warning: default_rpd_warning(),
            critical: default_rpd_critical(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub enabled: Option<bool>,
    pub endpoint: Option<String>,
    pub api_key_env: Option<String>,
    pub credential_source: Option<CredentialSource>,
    pub credential_file: Option<PathBuf>,
    #[serde(default = "default_usage_path")]
    pub usage_path: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            enabled: None,
            endpoint: None,
            api_key_env: None,
            credential_source: None,
            credential_file: None,
            usage_path: default_usage_path(),
        }
    }
}

impl ProviderConfig {
    pub fn resolve_api_key(&self) -> Result<Option<String>, ProviderError> {
        let Some(variable) = self.api_key_env.as_deref() else {
            return Ok(None);
        };
        match self.credential_source {
            None => {
                return std::env::var(variable).map(Some).map_err(|_| {
                    ProviderError::Configuration(format!(
                        "environment variable {variable} is not set"
                    ))
                });
            }
            Some(CredentialSource::Bashrc) => {
                if let Ok(value) = std::env::var(variable) {
                    return Ok(Some(value));
                }
            }
            Some(CredentialSource::Dotenv | CredentialSource::ManualKeyEntry) => {}
        }
        let Some(path) = self.credential_file.as_deref() else {
            return Err(ProviderError::Configuration(format!(
                "environment variable {variable} is not set"
            )));
        };
        let content = fs::read_to_string(path).map_err(|error| {
            ProviderError::Configuration(format!("{}: {error}", path.display()))
        })?;
        parse_environment_value(&content, variable)
            .map(Some)
            .ok_or_else(|| {
                ProviderError::Configuration(format!(
                    "{variable} was not found in {}",
                    path.display()
                ))
            })
    }
}

fn parse_environment_value(content: &str, variable: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let line = line.trim().strip_prefix("export ").unwrap_or(line.trim());
        let (name, value) = line.split_once('=')?;
        if name.trim() != variable {
            return None;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .or_else(|| {
                value
                    .strip_prefix('\'')
                    .and_then(|value| value.strip_suffix('\''))
            })
            .unwrap_or(value);
        (!value.is_empty()).then(|| value.to_owned())
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialSource {
    Bashrc,
    Dotenv,
    ManualKeyEntry,
}

fn default_usage_path() -> String {
    "/usage".into()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
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
            instruments: InstrumentConfig::default(),
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
fn default_rpm_max() -> f64 {
    10_000.0
}
fn default_rpm_redline() -> f64 {
    8_500.0
}
fn default_scale_mode() -> String {
    "fixed".into()
}
fn default_rpd_limit() -> f64 {
    100_000.0
}
fn default_rpd_warning() -> f64 {
    0.75
}
fn default_rpd_critical() -> f64 {
    0.90
}
fn default_provider_cycle_seconds() -> u64 {
    4
}
fn default_dashboard_sample_seconds() -> u64 {
    15
}
fn default_network_sample_millis() -> u64 {
    250
}
fn default_network_enabled() -> bool {
    true
}
fn default_network_bytes_per_token() -> f64 {
    4.0
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
        let local = PathBuf::from("ingauge.toml");
        if local.exists() {
            return Some(local);
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

    pub fn save(&self, path: &Path) -> Result<(), ProviderError> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|error| {
                ProviderError::Configuration(format!("{}: {error}", parent.display()))
            })?;
        }
        let rendered = toml::to_string_pretty(self)
            .map_err(|error| ProviderError::Configuration(error.to_string()))?;
        let temporary = path.with_extension("toml.tmp");
        write_private(&temporary, rendered.as_bytes())?;
        fs::rename(&temporary, path)
            .map_err(|error| ProviderError::Configuration(format!("{}: {error}", path.display())))
    }
}

pub fn write_private(path: &Path, content: &[u8]) -> Result<(), ProviderError> {
    use std::io::Write as _;
    let mut options = fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| ProviderError::Configuration(format!("{}: {error}", path.display())))?;
    file.write_all(content)
        .map_err(|error| ProviderError::Configuration(format!("{}: {error}", path.display())))
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

    #[test]
    fn credential_sources_resolve_without_exposing_values_in_config() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let dotenv = directory.path().join("provider.env");
        write_private(&dotenv, b"IGNORED=x\nTEST_PROVIDER_KEY='secret-value'\n")
            .expect("write private dotenv");
        let provider = ProviderConfig {
            api_key_env: Some("TEST_PROVIDER_KEY".into()),
            credential_source: Some(CredentialSource::Dotenv),
            credential_file: Some(dotenv),
            ..ProviderConfig::default()
        };
        assert!(matches!(
            provider.resolve_api_key(),
            Ok(Some(value)) if value == "secret-value"
        ));

        let mut config = Config::default();
        config.providers.insert("fixture".into(), provider);
        let path = directory.path().join("ingauge.toml");
        config.save(&path).expect("save config");
        let saved = fs::read_to_string(path).expect("read config");
        assert!(saved.contains("credential_source = \"dotenv\""));
        assert!(!saved.contains("secret-value"));
    }
}
