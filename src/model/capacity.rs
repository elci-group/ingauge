// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use super::{Confidence, Metric, ModelId, ProviderId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Quota {
    pub metric: Metric,
    pub used: Option<u64>,
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
    pub reset_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConsumptionRate {
    pub tokens_per_minute: Option<f64>,
    pub requests_per_minute: Option<f64>,
}

/// Telemetry used by the classic sports-car terminal panel.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelTelemetry {
    pub fuel_used: Option<u64>,
    pub fuel_limit: Option<u64>,
    pub tokens_used: Option<u64>,
    pub output_tokens: Option<u64>,
    pub tpm_limit: Option<f64>,
    pub responses: Option<u64>,
    pub rpm_limit: Option<f64>,
    pub rpd: Option<u64>,
    pub rpd_limit: Option<u64>,
    /// Current inference activity, expressed as requests per minute.
    pub rpm: Option<f64>,
    /// Canonical token throughput, expressed as tokens per minute.
    pub tpm: Option<f64>,
    /// Cumulative input tokens, for the lifetime odometer.
    pub lifetime_input_tokens: Option<u64>,
    /// Cumulative output tokens, for the lifetime odometer.
    pub lifetime_output_tokens: Option<u64>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapacityState {
    Healthy,
    Moderate,
    Constrained,
    Critical,
    Exhausted,
    Recovering,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CapacitySnapshot {
    pub provider: ProviderId,
    pub model: Option<ModelId>,
    pub available: bool,
    pub remaining: Vec<Quota>,
    pub utilisation: Vec<f64>,
    pub consumption_rate: Option<ConsumptionRate>,
    pub telemetry: ModelTelemetry,
    pub next_reset: Option<DateTime<Utc>>,
    pub exhaustion: Option<DateTime<Utc>>,
    pub headroom: Option<f64>,
    pub state: CapacityState,
    pub confidence: Confidence,
    pub observed_at: DateTime<Utc>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CapacityEvent {
    pub provider: ProviderId,
    pub model: Option<ModelId>,
    pub at: DateTime<Utc>,
    pub kind: CapacityEventKind,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CapacityEventKind {
    QuotaReset,
    ExpectedRecovery,
    ProjectedExhaustion,
}
