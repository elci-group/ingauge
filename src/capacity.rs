// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use crate::model::*;
use chrono::{DateTime, Duration, Utc};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct CapacityPolicy {
    pub moderate: f64,
    pub constrained: f64,
    pub critical: f64,
}

impl Default for CapacityPolicy {
    fn default() -> Self {
        Self {
            moderate: 0.7,
            constrained: 0.3,
            critical: 0.1,
        }
    }
}

pub fn snapshots(observations: &[Observation]) -> Vec<CapacitySnapshot> {
    snapshots_at(observations, Utc::now(), CapacityPolicy::default())
}

pub fn snapshots_at(
    observations: &[Observation],
    now: DateTime<Utc>,
    policy: CapacityPolicy,
) -> Vec<CapacitySnapshot> {
    let mut groups: BTreeMap<(ProviderId, Option<ModelId>), Vec<&Observation>> = BTreeMap::new();
    for observation in observations {
        groups
            .entry((observation.provider.clone(), observation.model.clone()))
            .or_default()
            .push(observation);
    }
    groups
        .into_iter()
        .map(|((provider, model), mut group)| {
            group.sort_by_key(|observation| observation.observed_at);
            derive_snapshot(provider, model, &group, now, policy)
        })
        .collect()
}

fn derive_snapshot(
    provider: ProviderId,
    model: Option<ModelId>,
    observations: &[&Observation],
    now: DateTime<Utc>,
    policy: CapacityPolicy,
) -> CapacitySnapshot {
    let latest = |metric| {
        observations
            .iter()
            .rev()
            .find(|o| o.metric == metric)
            .copied()
    };
    let integer = |metric| latest(metric).and_then(|o| o.value.as_u64());
    let decimal = |metric| {
        latest(metric)
            .and_then(|o| o.value.as_f64())
            .filter(|v| v.is_finite())
    };
    let reset = latest(Metric::ResetAt).and_then(|o| match o.value {
        MetricValue::Timestamp(value) => Some(value),
        _ => None,
    });

    let mut quotas = Vec::new();
    let token_used = integer(Metric::Tokens);
    let token_limit = integer(Metric::TokenLimit);
    let token_remaining = integer(Metric::RemainingTokens).or_else(|| {
        token_limit
            .zip(token_used)
            .map(|(limit, used)| limit.saturating_sub(used))
    });
    if token_used.is_some() || token_limit.is_some() || token_remaining.is_some() {
        quotas.push(Quota {
            metric: Metric::Tokens,
            used: token_used,
            limit: token_limit,
            remaining: token_remaining,
            reset_at: reset,
        });
    }
    let request_used = integer(Metric::Requests);
    let request_limit = integer(Metric::RequestLimit);
    let request_remaining = integer(Metric::RemainingRequests).or_else(|| {
        request_limit
            .zip(request_used)
            .map(|(limit, used)| limit.saturating_sub(used))
    });
    if request_used.is_some() || request_limit.is_some() || request_remaining.is_some() {
        quotas.push(Quota {
            metric: Metric::Requests,
            used: request_used,
            limit: request_limit,
            remaining: request_remaining,
            reset_at: reset,
        });
    }

    let utilisation: Vec<f64> = quotas
        .iter()
        .filter_map(|quota| {
            quota
                .limit
                .zip(quota.remaining)
                .and_then(|(limit, remaining)| {
                    (limit > 0).then_some(1.0 - (remaining as f64 / limit as f64).clamp(0.0, 1.0))
                })
        })
        .collect();
    let headroom = utilisation
        .iter()
        .copied()
        .map(|value| 1.0 - value)
        .reduce(f64::min);
    let tokens_per_minute = decimal(Metric::TokensPerMinute).filter(|rate| *rate >= 0.0);
    let requests_per_minute = decimal(Metric::RequestsPerMinute).filter(|rate| *rate >= 0.0);
    let exhaustion = token_remaining
        .zip(tokens_per_minute)
        .and_then(|(remaining, rate)| {
            if rate <= 0.0 {
                return None;
            }
            let seconds = remaining as f64 / rate * 60.0;
            if !seconds.is_finite() || seconds > i64::MAX as f64 {
                return None;
            }
            now.checked_add_signed(Duration::seconds(seconds as i64))
        })
        .filter(|projected| reset.is_none_or(|reset_at| *projected < reset_at));
    let state = classify(
        headroom,
        quotas.iter().any(|quota| quota.remaining == Some(0)),
        policy,
    );

    CapacitySnapshot {
        provider,
        model,
        available: true,
        remaining: quotas,
        utilisation,
        consumption_rate: (tokens_per_minute.is_some() || requests_per_minute.is_some()).then_some(
            ConsumptionRate {
                tokens_per_minute,
                requests_per_minute,
            },
        ),
        next_reset: reset,
        exhaustion,
        headroom,
        state,
        confidence: observations
            .iter()
            .map(|o| o.confidence)
            .max()
            .unwrap_or(Confidence::Unknown),
        observed_at: observations
            .iter()
            .map(|o| o.observed_at)
            .max()
            .unwrap_or(now),
        error: None,
    }
}

fn classify(headroom: Option<f64>, exhausted: bool, policy: CapacityPolicy) -> CapacityState {
    if exhausted {
        return CapacityState::Exhausted;
    }
    match headroom {
        None => CapacityState::Unknown,
        Some(value) if value < policy.critical => CapacityState::Critical,
        Some(value) if value < policy.constrained => CapacityState::Constrained,
        Some(value) if value < policy.moderate => CapacityState::Moderate,
        Some(_) => CapacityState::Healthy,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn observation(metric: Metric, value: MetricValue) -> Observation {
        Observation {
            provider: "test".into(),
            model: Some("model".into()),
            metric,
            value,
            observed_at: DateTime::from_timestamp(1_700_000_000, 0)
                .expect("valid fixture timestamp"),
            source: ObservationSource::Fixture,
            confidence: Confidence::High,
        }
    }

    #[test]
    fn unknown_is_not_zero() {
        let result = snapshots_at(
            &[observation(Metric::Tokens, MetricValue::Integer(10))],
            Utc::now(),
            CapacityPolicy::default(),
        );
        assert_eq!(result[0].headroom, None);
        assert_eq!(result[0].state, CapacityState::Unknown);
    }

    #[test]
    fn tightest_quota_controls_state() {
        let observations = [
            observation(Metric::TokenLimit, MetricValue::Integer(100)),
            observation(Metric::RemainingTokens, MetricValue::Integer(80)),
            observation(Metric::RequestLimit, MetricValue::Integer(100)),
            observation(Metric::RemainingRequests, MetricValue::Integer(5)),
        ];
        let result = snapshots_at(&observations, Utc::now(), CapacityPolicy::default());
        assert!(result[0]
            .headroom
            .is_some_and(|value| (value - 0.05).abs() < 1e-9));
        assert_eq!(result[0].state, CapacityState::Critical);
    }

    #[test]
    fn fractional_rate_is_preserved() {
        let observations = [
            observation(Metric::RemainingTokens, MetricValue::Integer(60)),
            observation(Metric::TokensPerMinute, MetricValue::Decimal(0.5)),
        ];
        let result = snapshots_at(&observations, Utc::now(), CapacityPolicy::default());
        assert_eq!(
            result[0]
                .consumption_rate
                .as_ref()
                .and_then(|r| r.tokens_per_minute),
            Some(0.5)
        );
    }
}
