// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use super::{ModelId, ProviderId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
