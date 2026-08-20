use crate::model::*;
use chrono::{DateTime, Utc};

pub fn events(snapshots: &[CapacitySnapshot]) -> Vec<CapacityEvent> {
    let mut events = Vec::new();
    for snapshot in snapshots {
        if let Some(at) = snapshot.next_reset {
            events.push(CapacityEvent {
                provider: snapshot.provider.clone(),
                model: snapshot.model.clone(),
                at,
                kind: CapacityEventKind::QuotaReset,
            });
            if snapshot.state == CapacityState::Exhausted {
                events.push(CapacityEvent {
                    provider: snapshot.provider.clone(),
                    model: snapshot.model.clone(),
                    at,
                    kind: CapacityEventKind::ExpectedRecovery,
                });
            }
        }
        if let Some(at) = snapshot.exhaustion {
            if snapshot.next_reset.is_none_or(|reset| at < reset) {
                events.push(CapacityEvent {
                    provider: snapshot.provider.clone(),
                    model: snapshot.model.clone(),
                    at,
                    kind: CapacityEventKind::ProjectedExhaustion,
                });
            }
        }
    }
    events.sort_by_key(|event| {
        (
            event.at,
            event.provider.clone(),
            event.model.clone(),
            event.kind,
        )
    });
    events
}

pub fn rolling_rate(samples: &[(DateTime<Utc>, u64)]) -> Option<f64> {
    regression_rate(samples, 2).map(|result| result.rate_per_minute)
}

pub fn regression_rate(
    samples: &[(DateTime<Utc>, u64)],
    minimum_samples: usize,
) -> Option<ForecastResult> {
    if samples.len() < minimum_samples.max(2) {
        return None;
    }
    let mut ordered = samples.to_vec();
    ordered.sort_by_key(|sample| sample.0);
    let reset_index = ordered
        .windows(2)
        .rposition(|pair| pair[1].1 < pair[0].1)
        .map_or(0, |index| index + 1);
    let segment = &ordered[reset_index..];
    if segment.len() < minimum_samples.max(2) {
        return None;
    }
    let origin = segment.first()?.0;
    let points: Vec<(f64, f64)> = segment
        .iter()
        .map(|(time, value)| {
            (
                ((*time - origin).num_milliseconds() as f64) / 60_000.0,
                *value as f64,
            )
        })
        .collect();
    let n = points.len() as f64;
    let mean_x = points.iter().map(|point| point.0).sum::<f64>() / n;
    let mean_y = points.iter().map(|point| point.1).sum::<f64>() / n;
    let denominator = points
        .iter()
        .map(|point| (point.0 - mean_x).powi(2))
        .sum::<f64>();
    if denominator <= f64::EPSILON {
        return None;
    }
    let rate = points
        .iter()
        .map(|point| (point.0 - mean_x) * (point.1 - mean_y))
        .sum::<f64>()
        / denominator;
    if !rate.is_finite() {
        return None;
    }
    let window_seconds = (segment.last()?.0 - origin).num_seconds();
    if window_seconds <= 0 {
        return None;
    }
    let confidence = match segment.len() {
        0..=2 => Confidence::Low,
        3..=4 => Confidence::Medium,
        5..=9 => Confidence::High,
        _ => Confidence::Authoritative,
    };
    Some(ForecastResult {
        rate_per_minute: rate,
        samples: segment.len(),
        window_seconds,
        confidence,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn regression_is_order_independent() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).expect("valid fixture timestamp");
        let samples = [
            (now + Duration::minutes(2), 20),
            (now, 0),
            (now + Duration::minutes(1), 10),
        ];
        assert_eq!(
            regression_rate(&samples, 3).map(|f| f.rate_per_minute),
            Some(10.0)
        );
    }

    #[test]
    fn reset_starts_a_new_segment() {
        let now = Utc::now();
        let samples = [
            (now, 100),
            (now + Duration::minutes(1), 200),
            (now + Duration::minutes(2), 0),
            (now + Duration::minutes(3), 10),
        ];
        assert_eq!(regression_rate(&samples, 2).map(|f| f.samples), Some(2));
    }

    #[test]
    fn events_include_recovery_and_are_ordered() {
        let now = Utc::now();
        let snapshots = [CapacitySnapshot {
            provider: "p".into(),
            model: None,
            available: true,
            remaining: Vec::new(),
            utilisation: Vec::new(),
            consumption_rate: None,
            next_reset: Some(now + Duration::hours(2)),
            exhaustion: Some(now + Duration::hours(1)),
            headroom: Some(0.0),
            state: CapacityState::Exhausted,
            confidence: Confidence::High,
            observed_at: now,
            error: None,
        }];
        let result = events(&snapshots);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].kind, CapacityEventKind::ProjectedExhaustion);
    }

    #[test]
    fn insufficient_or_flat_time_series_has_no_rate() {
        let now = Utc::now();
        assert!(regression_rate(&[(now, 1)], 2).is_none());
        assert!(regression_rate(&[(now, 1), (now, 2)], 2).is_none());
    }
}
