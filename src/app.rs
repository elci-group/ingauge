// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use crate::{
    capacity::{self, CapacityPolicy},
    config::{parse_duration, Config, ProviderConfig},
    error::AppError,
    forecast,
    model::*,
    providers::{GroqAdapter, HarnessAdapter, ProbeContext, ProviderAdapter, ProviderError},
    store::Store,
};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs::File, io::BufWriter, path::Path, time::Duration};

pub const JSON_SCHEMA_VERSION: u16 = 1;
type SeriesByCapacity = BTreeMap<(ProviderId, Option<ModelId>), Vec<(DateTime<Utc>, u64)>>;

#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub schema_version: u16,
    pub version: &'static str,
    pub command: &'static str,
    pub generated_at: DateTime<Utc>,
    pub data: Option<T>,
    pub warnings: Vec<String>,
    pub errors: Vec<crate::error::ErrorBody>,
}

impl<T: Serialize> Envelope<T> {
    pub fn success(command: &'static str, data: T) -> Self {
        Self {
            schema_version: JSON_SCHEMA_VERSION,
            version: env!("CARGO_PKG_VERSION"),
            command,
            generated_at: Utc::now(),
            data: Some(data),
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct App {
    pub config: Config,
    client: Client,
}

impl App {
    pub fn new(config: Config) -> Result<Self, AppError> {
        config.validate().map_err(config_error)?;
        let connect = parse_duration(&config.general.connect_timeout).map_err(config_error)?;
        let request = parse_duration(&config.general.request_timeout).map_err(config_error)?;
        let client = Client::builder()
            .connect_timeout(connect)
            .timeout(request)
            .user_agent(concat!("ingauge/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| AppError::Configuration(error.to_string()))?;
        Ok(Self { config, client })
    }

    pub fn database_path(&self) -> Result<&Path, AppError> {
        self.config.general.database.as_deref().ok_or_else(|| {
            AppError::Configuration(
                "general.database is required for history, daemon, health, and db commands".into(),
            )
        })
    }

    pub fn open_store(&self) -> Result<Store, AppError> {
        Ok(Store::open(self.database_path()?)?)
    }

    pub async fn probe(&self) -> Result<Vec<Observation>, AppError> {
        let providers: Vec<(String, ProviderConfig)> = self
            .config
            .providers
            .iter()
            .filter(|(_, provider)| provider.enabled.unwrap_or(true))
            .map(|(name, provider)| (name.clone(), provider.clone()))
            .collect();
        if providers.is_empty() {
            return Ok(Vec::new());
        }
        let mut observations = Vec::new();
        let mut successes = 0_usize;
        let mut last_error = None;
        for (name, provider) in providers {
            let started = Utc::now();
            match self.probe_target(&name, &provider).await {
                Ok(mut target_observations) => {
                    successes += 1;
                    if let Ok(store) = self.open_store() {
                        store.insert_batch(&target_observations)?;
                        store.record_probe(&name, started, Utc::now(), Ok(()))?;
                    }
                    observations.append(&mut target_observations);
                }
                Err(error) => {
                    tracing::debug!(
                        event = "capacity_target_failed",
                        target = name,
                        error_code = provider_error_code(&error),
                        error = %error,
                        "capacity target unavailable"
                    );
                    if let Ok(store) = self.open_store() {
                        let failure = Option::<()>::None.ok_or(provider_error_code(&error));
                        tracing::debug!(
                            event = "probe_failure_record_started",
                            target = name,
                            "persisting probe failure"
                        );
                        if let Err(record_error) =
                            store.record_probe(&name, started, Utc::now(), failure)
                        {
                            tracing::warn!(
                                event = "probe_failure_record_failed",
                                target = name,
                                error = %record_error,
                                "failed to persist probe failure"
                            );
                        }
                    }
                    last_error = Some(error);
                }
            }
        }
        if successes > 0 {
            Ok(observations)
        } else {
            tracing::debug!(
                event = "all_capacity_targets_failed",
                "no configured capacity target succeeded"
            );
            Err(last_error
                .unwrap_or_else(|| ProviderError::Unknown("no target was probed".into()))
                .into())
        }
    }

    async fn probe_target(
        &self,
        name: &str,
        provider: &ProviderConfig,
    ) -> Result<Vec<Observation>, ProviderError> {
        let endpoint = provider
            .endpoint
            .clone()
            .unwrap_or_else(|| "http://127.0.0.1:3000".into());
        let token = provider.resolve_api_key()?;
        let id = ProviderId::new(name).map_err(ProviderError::Configuration)?;
        let harness_adapter = HarnessAdapter {
            id: id.clone(),
            endpoint,
            usage_path: provider.usage_path.clone(),
        };
        let groq_adapter = GroqAdapter {
            id,
            endpoint: harness_adapter.endpoint.clone(),
            usage_path: harness_adapter.usage_path.clone(),
        };
        let direct_groq = name.eq_ignore_ascii_case("groq")
            && reqwest::Url::parse(&harness_adapter.endpoint)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .is_some_and(|host| host == "api.groq.com");
        let mut last_error = None;
        for attempt in 1..=self.config.general.max_attempts {
            let context = ProbeContext {
                client: self.client.clone(),
                now: Utc::now(),
                max_response_bytes: self.config.general.max_response_bytes,
                max_records: self.config.general.max_records,
                bearer_token: token.clone(),
            };
            let adapter_name = if direct_groq { "groq_api" } else { "harness" };
            let span = tracing::info_span!("provider_probe", provider = adapter_name, attempt);
            let _entered = span.enter();
            let result = if direct_groq {
                groq_adapter.probe(&context).await
            } else {
                harness_adapter.probe(&context).await
            };
            match result {
                Ok(snapshot) => {
                    tracing::info!(
                        observations = snapshot.observations.len(),
                        "provider probe completed"
                    );
                    return Ok(snapshot.observations);
                }
                Err(error) if error.retryable() && attempt < self.config.general.max_attempts => {
                    tracing::warn!(error_code = ?error, "transient provider probe failure");
                    last_error = Some(error);
                    tokio::time::sleep(Duration::from_millis(100 * u64::from(attempt))).await;
                }
                Err(error) => {
                    tracing::debug!(event = "provider_probe_failed", error_code = provider_error_code(&error), error = %error, "provider probe failed");
                    return Err(error);
                }
            }
        }
        let error = last_error.unwrap_or(ProviderError::Unknown(
            "retry loop ended unexpectedly".into(),
        ));
        tracing::debug!(event = "provider_retry_exhausted", error = %error, "provider retries exhausted");
        Err(error)
    }

    pub fn snapshots(
        &self,
        observations: &[Observation],
        now: DateTime<Utc>,
    ) -> Vec<CapacitySnapshot> {
        capacity::snapshots_at(
            observations,
            now,
            CapacityPolicy {
                moderate: self.config.forecast.moderate_threshold,
                constrained: self.config.forecast.constrained_threshold,
                critical: self.config.forecast.critical_threshold,
            },
        )
    }

    pub async fn status(&self, refresh: bool) -> Result<Value, AppError> {
        let mut telemetry_error = None;
        let observations = if refresh {
            self.probe().await.unwrap_or_else(|error| {
                telemetry_error = Some(error.to_string());
                Vec::new()
            })
        } else if self.config.general.database.is_some() {
            self.open_store()?.latest()?
        } else {
            self.probe().await.unwrap_or_else(|error| {
                telemetry_error = Some(error.to_string());
                Vec::new()
            })
        };
        let snapshots = self.snapshots(&observations, Utc::now());
        let events = forecast::events(&snapshots);
        let configured_providers = self
            .config
            .providers
            .iter()
            .filter(|(_, provider)| provider.enabled.unwrap_or(true))
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        Ok(json!({
            "snapshots": snapshots,
            "events": events,
            "instruments": self.config.instruments,
            "configured_providers": configured_providers,
            "telemetry_error": telemetry_error,
        }))
    }

    pub fn history(
        &self,
        provider: Option<&str>,
        model: Option<&str>,
        since: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<Vec<Observation>, AppError> {
        Ok(self
            .open_store()?
            .history(provider, model, None, since, limit)?)
    }

    pub fn forecast(&self, provider: Option<&str>, model: Option<&str>) -> Result<Value, AppError> {
        let window = parse_duration(&self.config.forecast.window).map_err(config_error)?;
        let since = Utc::now()
            - chrono::Duration::from_std(window)
                .map_err(|error| AppError::Configuration(error.to_string()))?;
        let history = self.open_store()?.history(
            provider,
            model,
            Some(Metric::Tokens),
            Some(since),
            100_000,
        )?;
        let mut groups: SeriesByCapacity = BTreeMap::new();
        for observation in history {
            if let Some(value) = observation.value.as_u64() {
                groups
                    .entry((observation.provider, observation.model))
                    .or_default()
                    .push((observation.observed_at, value));
            }
        }
        let results: Vec<Value> = groups
            .into_iter()
            .filter_map(|((provider, model), samples)| {
                forecast::regression_rate(&samples, self.config.forecast.minimum_samples).map(
                    |result| json!({ "provider": provider, "model": model, "forecast": result }),
                )
            })
            .collect();
        Ok(json!(results))
    }

    pub fn export_padagonia(
        &self,
        output: &Path,
        since: Option<DateTime<Utc>>,
    ) -> Result<usize, AppError> {
        let observations = self
            .open_store()?
            .history(None, None, None, since, 100_000)?;
        let snapshots = self.snapshots(&observations, Utc::now());
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut provider_ids = BTreeMap::new();
        let mut next_id = 1_u64;
        for snapshot in &snapshots {
            let provider_id = *provider_ids.entry(snapshot.provider.clone()).or_insert_with(|| {
                let id = allocate_id(&mut next_id);
                nodes.push(json!({"id":id,"label":"Provider","properties":{"name":snapshot.provider.as_str()}})); id
            });
            let model_id = allocate_id(&mut next_id);
            nodes.push(json!({"id":model_id,"label":"ModelCapacity","properties":{
                "model":snapshot.model.as_ref().map(ModelId::as_str), "state":snapshot.state,
                "headroom":snapshot.headroom, "confidence":snapshot.confidence, "observed_at":snapshot.observed_at
            }}));
            edges.push(json!({"id":edges.len()+1,"src":provider_id,"dst":model_id,"label":"reports_capacity","properties":{}}));
        }
        let projection = json!({"schema_version":1,"nodes":nodes,"edges":edges});
        serde_json::to_writer_pretty(BufWriter::new(File::create(output)?), &projection)?;
        Ok(observations.len())
    }
}

fn allocate_id(next: &mut u64) -> u64 {
    let allocated = *next;
    *next = next.saturating_add(1);
    allocated
}

fn provider_error_code(error: &ProviderError) -> &'static str {
    match error {
        ProviderError::Authentication => "authentication",
        ProviderError::RateLimited => "rate_limited",
        ProviderError::Network(_) => "network",
        ProviderError::Timeout => "timeout",
        ProviderError::InvalidResponse(_) => "invalid_response",
        ProviderError::Unsupported => "unsupported",
        ProviderError::Configuration(_) => "configuration",
        ProviderError::Unknown(_) => "unknown",
    }
}

fn config_error(error: ProviderError) -> AppError {
    AppError::Configuration(error.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn configured_app() -> (tempfile::TempDir, App) {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.general.database = Some(directory.path().join("usage.db"));
        (directory, App::new(config).unwrap())
    }

    #[tokio::test]
    async fn empty_database_status_and_forecast_are_stable() {
        let (_directory, app) = configured_app();
        let status = app.status(false).await.unwrap();
        assert_eq!(status["snapshots"].as_array().map(Vec::len), Some(0));
        assert_eq!(app.forecast(None, None).unwrap(), json!([]));
    }

    #[test]
    fn database_and_schema_errors_are_configuration_errors() {
        let app = App::new(Config::default()).unwrap();
        assert_eq!(
            app.database_path().unwrap_err().code(),
            "configuration_error"
        );

        let invalid = Config {
            schema_version: 99,
            ..Config::default()
        };
        assert!(matches!(
            App::new(invalid),
            Err(error) if error.code() == "configuration_error"
        ));
    }

    #[test]
    fn success_envelope_is_versioned_and_has_no_errors() {
        let envelope = Envelope::success("test", json!({"ok": true}));
        assert_eq!(envelope.schema_version, JSON_SCHEMA_VERSION);
        assert_eq!(envelope.command, "test");
        assert!(envelope.errors.is_empty());
    }
}
