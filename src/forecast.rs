use crate::model::*;
use chrono::{DateTime, Utc};
pub fn events(snapshots: &[CapacitySnapshot]) -> Vec<CapacityEvent> {
    let mut e = Vec::new();
    for s in snapshots {
        if let Some(at) = s.next_reset {
            e.push(CapacityEvent {
                provider: s.provider.clone(),
                model: s.model.clone(),
                at,
                kind: CapacityEventKind::QuotaReset,
            })
        }
        if let Some(at) = s.exhaustion {
            if at < s.next_reset.unwrap_or(at) {
                e.push(CapacityEvent {
                    provider: s.provider.clone(),
                    model: s.model.clone(),
                    at,
                    kind: CapacityEventKind::ProjectedExhaustion,
                })
            }
        }
    }
    e.sort_by_key(|x| x.at);
    e
}
pub fn rolling_rate(samples: &[(DateTime<Utc>, u64)]) -> Option<f64> {
    if samples.len() < 2 {
        return None;
    }
    let (a, av) = samples.first()?;
    let (b, bv) = samples.last()?;
    let mins = (*b - *a).num_seconds() as f64 / 60.;
    (mins > 0.).then_some((*bv as f64 - *av as f64) / mins)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rate() {
        let t = Utc::now();
        assert_eq!(
            rolling_rate(&[(t, 0), (t + chrono::Duration::minutes(100), 10000)]),
            Some(100.)
        );
    }
}
