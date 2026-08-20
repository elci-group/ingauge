use crate::model::*;
use chrono::Utc;
use std::collections::HashMap;
pub fn snapshots(observations: &[Observation]) -> Vec<CapacitySnapshot> {
    let mut groups: HashMap<(ProviderId, Option<ModelId>), Vec<&Observation>> = HashMap::new();
    for o in observations {
        groups
            .entry((o.provider.clone(), o.model.clone()))
            .or_default()
            .push(o)
    }
    groups
        .into_iter()
        .map(|((provider, model), os)| {
            let get = |m| os.iter().find(|o| o.metric == m);
            let int = |m| {
                get(m).and_then(|o| match &o.value {
                    MetricValue::Integer(v) => Some(*v),
                    _ => None,
                })
            };
            let reset = get(Metric::ResetAt).and_then(|o| match &o.value {
                MetricValue::Timestamp(v) => Some(*v),
                _ => None,
            });
            let used = int(Metric::Tokens);
            let limit = int(Metric::TokenLimit);
            let remaining = int(Metric::RemainingTokens)
                .or_else(|| limit.zip(used).map(|(l, u)| l.saturating_sub(u)));
            let headroom = remaining
                .zip(limit)
                .map(|(r, l)| if l == 0 { 0.0 } else { r as f64 / l as f64 })
                .unwrap_or(0.0);
            let rate = int(Metric::TokensPerMinute).map(|v| v as f64);
            let exhaustion = remaining.zip(rate).and_then(|(r, v)| {
                if v > 0.0 {
                    Some(Utc::now() + chrono::Duration::seconds((r as f64 / v * 60.0) as i64))
                } else {
                    None
                }
            });
            let state = if !os.is_empty() {
                if remaining == Some(0) {
                    CapacityState::Exhausted
                } else if headroom < 0.1 {
                    CapacityState::Critical
                } else if headroom < 0.3 {
                    CapacityState::Constrained
                } else if headroom < 0.7 {
                    CapacityState::Moderate
                } else {
                    CapacityState::Healthy
                }
            } else {
                CapacityState::Unknown
            };
            CapacitySnapshot {
                provider,
                model,
                available: true,
                remaining: vec![Quota {
                    metric: Metric::Tokens,
                    used,
                    limit,
                    remaining,
                    reset_at: reset,
                }],
                utilisation: if limit.is_some() {
                    vec![1.0 - headroom]
                } else {
                    vec![]
                },
                consumption_rate: rate.map(|v| ConsumptionRate {
                    tokens_per_minute: Some(v),
                    requests_per_minute: int(Metric::RequestsPerMinute).map(|x| x as f64),
                }),
                next_reset: reset,
                exhaustion,
                headroom,
                state,
                confidence: os
                    .iter()
                    .map(|o| o.confidence.clone())
                    .max()
                    .unwrap_or(Confidence::Unknown),
                observed_at: os
                    .iter()
                    .map(|o| o.observed_at)
                    .max()
                    .unwrap_or_else(Utc::now),
                error: None,
            }
        })
        .collect()
}
