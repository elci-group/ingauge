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
