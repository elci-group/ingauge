use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ProviderId(pub String);
impl From<&str> for ProviderId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}
impl From<String> for ProviderId {
    fn from(s: String) -> Self {
        Self(s)
    }
}
impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ModelId(pub String);
impl From<&str> for ModelId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    Requests,
    RequestsPerMinute,
    Tokens,
    TokensPerMinute,
    InputTokens,
    OutputTokens,
    TokenLimit,
    RequestLimit,
    RemainingTokens,
    RemainingRequests,
    DailyUsage,
    DailyLimit,
    MonthlyUsage,
    MonthlyLimit,
    ResetAt,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MetricValue {
    Integer(u64),
    Timestamp(DateTime<Utc>),
    Text(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ObservationSource {
    Harness,
    ProviderApi,
    Inferred,
    Fixture,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Unknown,
    Estimated,
    Low,
    Medium,
    High,
    Authoritative,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Observation {
    pub provider: ProviderId,
    pub model: Option<ModelId>,
    pub metric: Metric,
    pub value: MetricValue,
    pub observed_at: DateTime<Utc>,
    pub source: ObservationSource,
    pub confidence: Confidence,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Quota {
    pub metric: Metric,
    pub used: Option<u64>,
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
    pub reset_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsumptionRate {
    pub tokens_per_minute: Option<f64>,
    pub requests_per_minute: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CapacitySnapshot {
    pub provider: ProviderId,
    pub model: Option<ModelId>,
    pub available: bool,
    pub remaining: Vec<Quota>,
    pub utilisation: Vec<f64>,
    pub consumption_rate: Option<ConsumptionRate>,
    pub next_reset: Option<DateTime<Utc>>,
    pub exhaustion: Option<DateTime<Utc>>,
    pub headroom: f64,
    pub state: CapacityState,
    pub confidence: Confidence,
    pub observed_at: DateTime<Utc>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CapacityEvent {
    pub provider: ProviderId,
    pub model: Option<ModelId>,
    pub at: DateTime<Utc>,
    pub kind: CapacityEventKind,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityEventKind {
    QuotaReset,
    ExpectedRecovery,
    ProjectedExhaustion,
}
