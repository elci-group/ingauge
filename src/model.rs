use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! identifier {
    ($name:ident) => {
        #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, String> {
                let value = value.into();
                if value.is_empty() || value.len() > 128 {
                    tracing::warn!(
                        event = "identifier_rejected",
                        identifier_type = stringify!($name),
                        reason = "length",
                        "identifier validation failed"
                    );
                    return Err("identifier length must be between 1 and 128 bytes".into());
                }
                if !value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':'))
                {
                    tracing::warn!(
                        event = "identifier_rejected",
                        identifier_type = stringify!($name),
                        reason = "characters",
                        "identifier validation failed"
                    );
                    return Err("identifier contains unsupported characters".into());
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

identifier!(ProviderId);
identifier!(ModelId);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    Decimal(f64),
    Timestamp(DateTime<Utc>),
    Text(String),
}

impl MetricValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Integer(value) => Some(*value as f64),
            Self::Decimal(value) if value.is_finite() => Some(*value),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservationSource {
    Harness,
    ProviderApi,
    Inferred,
    Fixture,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Unknown,
    Estimated,
    Low,
    Medium,
    High,
    Authoritative,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Observation {
    pub provider: ProviderId,
    pub model: Option<ModelId>,
    pub metric: Metric,
    pub value: MetricValue,
    pub observed_at: DateTime<Utc>,
    pub source: ObservationSource,
    pub confidence: Confidence,
}

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ForecastResult {
    pub rate_per_minute: f64,
    pub samples: usize,
    pub window_seconds: i64,
    pub confidence: Confidence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_and_metric_values_validate() {
        assert!(ProviderId::new("valid-id").is_ok());
        assert!(ProviderId::new("").is_err());
        assert!(ModelId::new("bad id").is_err());
        assert_eq!(MetricValue::Integer(2).as_f64(), Some(2.0));
        assert_eq!(MetricValue::Decimal(f64::NAN).as_f64(), None);
        assert_eq!(MetricValue::Text("x".into()).as_u64(), None);
    }
}
