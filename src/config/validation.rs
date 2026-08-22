// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use super::{parse_duration, Config, ProviderConfig, CONFIG_SCHEMA_VERSION};
use crate::{model::ProviderId, providers::ProviderError};
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
const MAX_RECORDS: usize = 1_000_000;

impl Config {
    pub fn validate(&self) -> Result<(), ProviderError> {
        validate_schema(self.schema_version)?;
        validate_durations(self)?;
        validate_general_bounds(self)?;
        validate_admission(self)?;
        validate_forecast(self)?;
        validate_providers(self)
    }
}

fn validate_schema(schema_version: u16) -> Result<(), ProviderError> {
    if schema_version == CONFIG_SCHEMA_VERSION {
        return Ok(());
    }
    reject(
        "schema_version",
        format!("unsupported schema_version {schema_version}; expected {CONFIG_SCHEMA_VERSION}"),
    )
}

fn validate_durations(config: &Config) -> Result<(), ProviderError> {
    let poll = parse_duration(&config.general.poll_interval)?;
    if !(Duration::from_secs(1)..=Duration::from_secs(86_400)).contains(&poll) {
        return reject("poll_interval", "poll_interval must be between 1s and 1d");
    }
    for duration in [
        &config.general.history_retention,
        &config.general.connect_timeout,
        &config.general.request_timeout,
        &config.forecast.window,
    ] {
        parse_duration(duration)?;
    }
    Ok(())
}

fn validate_general_bounds(config: &Config) -> Result<(), ProviderError> {
    if !(1..=MAX_RESPONSE_BYTES).contains(&config.general.max_response_bytes) {
        return reject(
            "max_response_bytes",
            "max_response_bytes must be between 1 and 67108864",
        );
    }
    if !(1..=MAX_RECORDS).contains(&config.general.max_records) {
        return reject("max_records", "max_records must be between 1 and 1000000");
    }
    if !(1..=10).contains(&config.general.max_attempts) {
        return reject("max_attempts", "max_attempts must be between 1 and 10");
    }
    Ok(())
}

fn validate_admission(config: &Config) -> Result<(), ProviderError> {
    if !config.admission.enabled {
        return Ok(());
    }
    if config.admission.max_concurrent == 0 {
        return reject(
            "admission.max_concurrent",
            "admission.max_concurrent must be at least 1",
        );
    }
    if config.admission.default_delay_ms == 0 {
        return reject(
            "admission.default_delay_ms",
            "admission.default_delay_ms must be at least 1",
        );
    }
    Ok(())
}

fn validate_forecast(config: &Config) -> Result<(), ProviderError> {
    if config.forecast.minimum_samples < 2 {
        return reject("minimum_samples", "minimum_samples must be at least 2");
    }
    let critical = config.forecast.critical_threshold;
    let constrained = config.forecast.constrained_threshold;
    let moderate = config.forecast.moderate_threshold;
    if 0.0 < critical && critical < constrained && constrained < moderate && moderate < 1.0 {
        return Ok(());
    }
    tracing::warn!(
        event = "configuration_rejected",
        field = "forecast_thresholds",
        critical,
        constrained,
        moderate,
        "configuration values invalid"
    );
    Err(ProviderError::Configuration(
        "thresholds must satisfy 0 < critical < constrained < moderate < 1".into(),
    ))
}

fn validate_providers(config: &Config) -> Result<(), ProviderError> {
    for (id, provider) in &config.providers {
        if provider.enabled.unwrap_or(true) {
            validate_provider(id, provider)?;
        }
    }
    Ok(())
}

fn validate_provider(id: &str, provider: &ProviderConfig) -> Result<(), ProviderError> {
    ProviderId::new(id.to_owned()).map_err(ProviderError::Configuration)?;
    let endpoint = provider
        .endpoint
        .as_deref()
        .unwrap_or("http://127.0.0.1:3000");
    let url = reqwest::Url::parse(endpoint).map_err(|error| {
        ProviderError::Configuration(format!("invalid endpoint for {id}: {error}"))
    })?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return reject(
            "endpoint",
            format!("{id} endpoint must use HTTPS unless it is loopback"),
        );
    }
    if provider
        .api_key_env
        .as_ref()
        .is_some_and(|name| !valid_environment_name(name))
    {
        return reject("api_key_env", format!("invalid api_key_env for {id}"));
    }
    if !provider.usage_path.starts_with('/')
        || provider.usage_path.contains('?')
        || provider.usage_path.contains('#')
    {
        return reject("usage_path", format!("invalid usage_path for {id}"));
    }
    Ok(())
}

fn valid_environment_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

fn reject(field: &'static str, message: impl Into<String>) -> Result<(), ProviderError> {
    let message = message.into();
    tracing::warn!(
        event = "configuration_rejected",
        field,
        "configuration value invalid"
    );
    Err(ProviderError::Configuration(message))
}
