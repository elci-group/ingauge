// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use crate::model::*;
use chrono::{DateTime, Utc};
use reqwest::{header, header::HeaderMap, Client};
use serde::{Deserialize, Serialize};
use std::future::Future;

#[derive(Debug, thiserror::Error, Clone, Serialize)]
#[serde(tag = "code", content = "message", rename_all = "snake_case")]
pub enum ProviderError {
    #[error("authentication failed")]
    Authentication,
    #[error("rate limited")]
    RateLimited,
    #[error("network request failed: {0}")]
    Network(String),
    #[error("request timed out")]
    Timeout,
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    #[error("unsupported provider")]
    Unsupported,
    #[error("configuration: {0}")]
    Configuration(String),
    #[error("unknown provider failure: {0}")]
    Unknown(String),
}

impl ProviderError {
    pub fn retryable(&self) -> bool {
        matches!(self, Self::RateLimited | Self::Network(_) | Self::Timeout)
    }
}

#[derive(Clone, Debug)]
pub struct ProbeContext {
    pub client: Client,
    pub now: DateTime<Utc>,
    pub max_response_bytes: usize,
    pub max_records: usize,
    pub bearer_token: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProviderSnapshot {
    pub provider: ProviderId,
    pub model: Option<ModelId>,
    pub observations: Vec<Observation>,
}

pub trait ProviderAdapter: Send + Sync {
    fn id(&self) -> ProviderId;
    fn probe(
        &self,
        context: &ProbeContext,
    ) -> impl Future<Output = Result<ProviderSnapshot, ProviderError>> + Send;
}

pub struct HarnessAdapter {
    pub id: ProviderId,
    pub endpoint: String,
    pub usage_path: String,
}

/// Reads Groq's authenticated rolling rate-limit headers without issuing an
/// inference request. Token headers describe the active TPM window; request
/// headers describe the daily allowance.
pub struct GroqAdapter {
    pub id: ProviderId,
    pub endpoint: String,
    pub usage_path: String,
}

impl ProviderAdapter for GroqAdapter {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    async fn probe(&self, context: &ProbeContext) -> Result<ProviderSnapshot, ProviderError> {
        let url = format!("{}{}", self.endpoint.trim_end_matches('/'), self.usage_path);
        let mut request = context.client.get(url);
        if let Some(token) = &context.bearer_token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let response = request.send().await.map_err(map_reqwest_error)?;
        validate_status(response.status().as_u16())?;
        if response
            .content_length()
            .is_some_and(|length| length > context.max_response_bytes as u64)
        {
            return Err(ProviderError::InvalidResponse(
                "response exceeds configured byte limit".into(),
            ));
        }
        let headers = response.headers().clone();
        let bytes = response.bytes().await.map_err(map_reqwest_error)?;
        if bytes.len() > context.max_response_bytes {
            return Err(ProviderError::InvalidResponse(
                "response exceeds configured byte limit".into(),
            ));
        }
        Ok(ProviderSnapshot {
            provider: self.id(),
            model: None,
            observations: groq_header_observations(&self.id, &headers, context.now),
        })
    }
}

fn validate_status(status: u16) -> Result<(), ProviderError> {
    match status {
        200..=299 => Ok(()),
        401 | 403 => Err(ProviderError::Authentication),
        429 => Err(ProviderError::RateLimited),
        status => Err(ProviderError::InvalidResponse(format!(
            "HTTP status {status}"
        ))),
    }
}

fn groq_header_observations(
    provider: &ProviderId,
    headers: &HeaderMap,
    now: DateTime<Utc>,
) -> Vec<Observation> {
    let header = |name: &'static str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
    };
    let token_limit = header("x-ratelimit-limit-tokens");
    let token_remaining = header("x-ratelimit-remaining-tokens");
    let request_limit = header("x-ratelimit-limit-requests");
    let request_remaining = header("x-ratelimit-remaining-requests");
    let token_usage = token_limit
        .zip(token_remaining)
        .map(|(limit, remaining)| limit.saturating_sub(remaining));
    let request_usage = request_limit
        .zip(request_remaining)
        .map(|(limit, remaining)| limit.saturating_sub(remaining));
    let mut observations = Vec::new();
    let mut push = |metric, value| {
        observations.push(Observation {
            provider: provider.clone(),
            model: None,
            metric,
            value,
            observed_at: now,
            source: ObservationSource::ProviderApi,
            confidence: Confidence::High,
        });
    };
    for (metric, value) in [
        (Metric::TokenLimit, token_limit),
        (Metric::RemainingTokens, token_remaining),
        (Metric::Tokens, token_usage),
        (Metric::DailyLimit, request_limit),
        (Metric::DailyUsage, request_usage),
    ] {
        if let Some(value) = value {
            push(metric, MetricValue::Integer(value));
        }
    }
    push(
        Metric::TokensPerMinute,
        MetricValue::Decimal(token_usage.unwrap_or(0) as f64),
    );
    observations
}

#[derive(Debug, Deserialize)]
struct UsageRecord {
    provider: String,
    model: Option<String>,
    used: Option<u64>,
    limit: Option<u64>,
    remaining: Option<u64>,
    requests: Option<u64>,
    request_limit: Option<u64>,
    remaining_requests: Option<u64>,
    reset_at: Option<DateTime<Utc>>,
    tokens_per_minute: Option<f64>,
    requests_per_minute: Option<f64>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    monthly_usage: Option<u64>,
    monthly_limit: Option<u64>,
    budget: Option<u64>,
    budget_limit: Option<u64>,
    daily_usage: Option<u64>,
    daily_limit: Option<u64>,
}

impl ProviderAdapter for HarnessAdapter {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    async fn probe(&self, context: &ProbeContext) -> Result<ProviderSnapshot, ProviderError> {
        let url = format!("{}{}", self.endpoint.trim_end_matches('/'), self.usage_path);
        let mut request = context.client.get(url);
        if let Some(token) = &context.bearer_token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let response = request.send().await.map_err(map_reqwest_error)?;
        match response.status().as_u16() {
            401 | 403 => {
                tracing::debug!(
                    event = "provider_response_rejected",
                    reason = "authentication",
                    "provider request failed"
                );
                return Err(ProviderError::Authentication);
            }
            429 => {
                tracing::debug!(
                    event = "provider_response_rejected",
                    reason = "rate_limited",
                    "provider request failed"
                );
                return Err(ProviderError::RateLimited);
            }
            status if !(200..300).contains(&status) => {
                tracing::debug!(
                    event = "provider_response_rejected",
                    status,
                    "provider returned unsuccessful status"
                );
                return Err(ProviderError::InvalidResponse(format!(
                    "HTTP status {status}"
                )));
            }
            _ => {}
        }
        if response
            .content_length()
            .is_some_and(|length| length > context.max_response_bytes as u64)
        {
            tracing::warn!(
                event = "provider_response_rejected",
                reason = "declared_body_too_large",
                "provider response rejected"
            );
            return Err(ProviderError::InvalidResponse(
                "response exceeds configured byte limit".into(),
            ));
        }
        let bytes = response.bytes().await.map_err(map_reqwest_error)?;
        if bytes.len() > context.max_response_bytes {
            tracing::warn!(
                event = "provider_response_rejected",
                reason = "body_too_large",
                bytes = bytes.len(),
                "provider response rejected"
            );
            return Err(ProviderError::InvalidResponse(
                "response exceeds configured byte limit".into(),
            ));
        }
        let observations = parse_usage(&bytes, context.now, context.max_records)?;
        Ok(ProviderSnapshot {
            provider: self.id(),
            model: None,
            observations,
        })
    }
}

fn map_reqwest_error(error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        ProviderError::Timeout
    } else {
        ProviderError::Network(error.without_url().to_string())
    }
}

pub fn parse_usage(
    input: &[u8],
    now: DateTime<Utc>,
    max_records: usize,
) -> Result<Vec<Observation>, ProviderError> {
    let records: Vec<UsageRecord> = serde_json::from_slice(input)
        .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
    if records.len() > max_records {
        tracing::warn!(
            event = "provider_response_rejected",
            reason = "record_limit",
            records = records.len(),
            "provider response rejected"
        );
        return Err(ProviderError::InvalidResponse(
            "response exceeds configured record limit".into(),
        ));
    }
    let mut output = Vec::with_capacity(records.len() * 7);
    for record in records {
        let provider = ProviderId::new(record.provider).map_err(ProviderError::InvalidResponse)?;
        let model = record
            .model
            .map(ModelId::new)
            .transpose()
            .map_err(ProviderError::InvalidResponse)?;
        let mut push = |metric, value| {
            output.push(Observation {
                provider: provider.clone(),
                model: model.clone(),
                metric,
                value,
                observed_at: now,
                source: ObservationSource::Harness,
                confidence: Confidence::Authoritative,
            })
        };
        if let Some(value) = record.used {
            push(Metric::Tokens, MetricValue::Integer(value));
        }
        if let Some(value) = record.limit {
            push(Metric::TokenLimit, MetricValue::Integer(value));
        }
        if let Some(value) = record.remaining {
            push(Metric::RemainingTokens, MetricValue::Integer(value));
        }
        for (metric, value) in [
            (Metric::Requests, record.requests),
            (Metric::RequestLimit, record.request_limit),
            (Metric::RemainingRequests, record.remaining_requests),
        ] {
            if let Some(value) = value {
                push(metric, MetricValue::Integer(value));
            }
        }
        if let Some(value) = record.reset_at {
            push(Metric::ResetAt, MetricValue::Timestamp(value));
        }
        if let Some(value) = record.tokens_per_minute {
            if !value.is_finite() || value < 0.0 {
                tracing::warn!(
                    event = "provider_response_rejected",
                    field = "tokens_per_minute",
                    "invalid provider metric"
                );
                return Err(ProviderError::InvalidResponse(
                    "tokens_per_minute must be finite and non-negative".into(),
                ));
            }
            push(Metric::TokensPerMinute, MetricValue::Decimal(value));
        }
        if let Some(value) = record.requests_per_minute {
            if !value.is_finite() || value < 0.0 {
                tracing::warn!(
                    event = "provider_response_rejected",
                    field = "requests_per_minute",
                    "invalid provider metric"
                );
                return Err(ProviderError::InvalidResponse(
                    "requests_per_minute must be finite and non-negative".into(),
                ));
            }
            push(Metric::RequestsPerMinute, MetricValue::Decimal(value));
        }
        for (metric, value) in [
            (Metric::InputTokens, record.input_tokens),
            (Metric::OutputTokens, record.output_tokens),
            (Metric::MonthlyUsage, record.monthly_usage),
            (Metric::MonthlyLimit, record.monthly_limit),
            (Metric::BudgetUsage, record.budget),
            (Metric::BudgetLimit, record.budget_limit),
            (Metric::DailyUsage, record.daily_usage),
            (Metric::DailyLimit, record.daily_limit),
        ] {
            if let Some(value) = value {
                push(metric, MetricValue::Integer(value));
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

    #[test]
    fn parser_preserves_fractional_rates_and_models() {
        let now = Utc::now();
        let input = br#"[{"provider":"groq","model":"m1","tokens_per_minute":0.5}]"#;
        let parsed = parse_usage(input, now, 10).expect("fixture should parse");
        assert_eq!(parsed[0].model.as_ref().map(ModelId::as_str), Some("m1"));
        assert_eq!(parsed[0].value, MetricValue::Decimal(0.5));
    }

    #[test]
    fn parser_enforces_record_and_number_bounds() {
        assert!(parse_usage(br#"[{},{}]"#, Utc::now(), 1).is_err());
        assert!(parse_usage(
            br#"[{"provider":"x","tokens_per_minute":-1}]"#,
            Utc::now(),
            1
        )
        .is_err());
    }

    #[test]
    fn parser_preserves_all_supported_fields() {
        let now = Utc::now();
        let input = br#"[{"provider":"groq","model":"m1","used":1,"limit":10,"remaining":9,"reset_at":"2026-08-20T12:00:00Z","tokens_per_minute":1.25,"requests_per_minute":2.5}]"#;
        let parsed = parse_usage(input, now, 1).expect("fixture should parse");
        assert_eq!(parsed.len(), 6);
        assert!(parsed.iter().any(|item| item.metric == Metric::ResetAt));
        assert!(parsed
            .iter()
            .any(|item| item.value == MetricValue::Decimal(2.5)));
    }

    #[test]
    fn parser_maps_sports_car_telemetry_fields() {
        let input = br#"[{"provider":"groq","model":"m1","output_tokens":40,"requests":2,"request_limit":10,"monthly_usage":25,"monthly_limit":100,"daily_usage":20,"daily_limit":100}]"#;
        let parsed = parse_usage(input, Utc::now(), 1).expect("fixture should parse");
        assert!(parsed
            .iter()
            .any(|item| item.metric == Metric::OutputTokens));
        assert!(parsed
            .iter()
            .any(|item| item.metric == Metric::RequestLimit));
        assert!(parsed
            .iter()
            .any(|item| item.metric == Metric::MonthlyLimit));
        assert!(parsed.iter().any(|item| item.metric == Metric::DailyLimit));
    }

    #[test]
    fn parser_rejects_invalid_identifiers_and_json() {
        assert!(parse_usage(br#"[{"provider":""}]"#, Utc::now(), 1).is_err());
        assert!(parse_usage(b"not-json", Utc::now(), 1).is_err());
    }

    #[test]
    fn only_transient_provider_errors_are_retryable() {
        assert!(ProviderError::Network("offline".into()).retryable());
        assert!(ProviderError::Timeout.retryable());
        assert!(ProviderError::RateLimited.retryable());
        assert!(!ProviderError::Authentication.retryable());
        assert!(!ProviderError::InvalidResponse("bad".into()).retryable());
    }

    #[test]
    fn groq_rate_headers_map_to_tpm_and_daily_resource_instruments() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-ratelimit-limit-tokens",
            HeaderValue::from_static("18000"),
        );
        headers.insert(
            "x-ratelimit-remaining-tokens",
            HeaderValue::from_static("17997"),
        );
        headers.insert(
            "x-ratelimit-limit-requests",
            HeaderValue::from_static("14400"),
        );
        headers.insert(
            "x-ratelimit-remaining-requests",
            HeaderValue::from_static("14370"),
        );
        let observations = groq_header_observations(&"groq".into(), &headers, Utc::now());
        assert!(observations.iter().any(|observation| {
            observation.metric == Metric::TokensPerMinute
                && observation.value == MetricValue::Decimal(3.0)
        }));
        assert!(observations.iter().any(|observation| {
            observation.metric == Metric::DailyUsage
                && observation.value == MetricValue::Integer(30)
        }));
        assert!(observations
            .iter()
            .all(|observation| observation.source == ObservationSource::ProviderApi));
    }

    #[test]
    fn groq_connectivity_without_rate_headers_reports_live_idle_tpm() {
        let observations = groq_header_observations(&"groq".into(), &HeaderMap::new(), Utc::now());
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].metric, Metric::TokensPerMinute);
        assert_eq!(observations[0].value, MetricValue::Decimal(0.0));
    }
}
