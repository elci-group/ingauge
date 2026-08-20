use crate::model::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error, Clone, Serialize)]
pub enum ProviderError {
    #[error("authentication failed")]
    Authentication,
    #[error("rate limited")]
    RateLimited,
    #[error("network: {0}")]
    Network(String),
    #[error("timeout")]
    Timeout,
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    #[error("unsupported")]
    Unsupported,
    #[error("configuration: {0}")]
    Configuration(String),
    #[error("unknown: {0}")]
    Unknown(String),
}
#[derive(Clone)]
pub struct ProbeContext {
    pub client: Client,
    pub now: DateTime<Utc>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ProviderSnapshot {
    pub provider: ProviderId,
    pub model: Option<ModelId>,
    pub observations: Vec<Observation>,
}
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn id(&self) -> ProviderId;
    async fn probe(&self, ctx: &ProbeContext) -> Result<ProviderSnapshot, ProviderError>;
}

pub struct HarnessAdapter {
    pub endpoint: String,
}
#[derive(Deserialize)]
struct UsageRecord {
    provider: String,
    model: Option<String>,
    used: Option<u64>,
    limit: Option<u64>,
    remaining: Option<u64>,
    reset_at: Option<DateTime<Utc>>,
    tokens_per_minute: Option<f64>,
    requests_per_minute: Option<f64>,
}
#[async_trait]
impl ProviderAdapter for HarnessAdapter {
    fn id(&self) -> ProviderId {
        "harness".into()
    }
    async fn probe(&self, ctx: &ProbeContext) -> Result<ProviderSnapshot, ProviderError> {
        let r = ctx
            .client
            .get(format!("{}/usage", self.endpoint.trim_end_matches('/')))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::Network(e.to_string())
                }
            })?;
        if r.status() == 401 || r.status() == 403 {
            return Err(ProviderError::Authentication);
        }
        if r.status() == 429 {
            return Err(ProviderError::RateLimited);
        }
        if !r.status().is_success() {
            return Err(ProviderError::InvalidResponse(r.status().to_string()));
        }
        let records: Vec<UsageRecord> = r
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
        let mut out = Vec::new();
        for x in records {
            let p: ProviderId = x.provider.into();
            let m = x.model.as_deref().map(Into::into);
            let q = |metric, value| Observation {
                provider: p.clone(),
                model: m.clone(),
                metric,
                value,
                observed_at: ctx.now,
                source: ObservationSource::Harness,
                confidence: Confidence::Authoritative,
            };
            if let Some(v) = x.used {
                out.push(q(Metric::Tokens, MetricValue::Integer(v)))
            }
            if let Some(v) = x.limit {
                out.push(q(Metric::TokenLimit, MetricValue::Integer(v)))
            }
            if let Some(v) = x.remaining {
                out.push(q(Metric::RemainingTokens, MetricValue::Integer(v)))
            }
            if let Some(v) = x.reset_at {
                out.push(q(Metric::ResetAt, MetricValue::Timestamp(v)))
            }
            if let Some(v) = x.tokens_per_minute {
                out.push(q(
                    Metric::TokensPerMinute,
                    MetricValue::Integer(v.max(0.0) as u64),
                ))
            }
            if let Some(v) = x.requests_per_minute {
                out.push(q(
                    Metric::RequestsPerMinute,
                    MetricValue::Integer(v.max(0.0) as u64),
                ))
            }
        }
        Ok(ProviderSnapshot {
            provider: self.id(),
            model: None,
            observations: out,
        })
    }
}
