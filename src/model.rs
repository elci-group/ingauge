mod capacity;
mod forecast;
mod identifier;
mod metric;

pub use capacity::{
    CapacityEvent, CapacityEventKind, CapacitySnapshot, CapacityState, ConsumptionRate, Quota,
};
pub use forecast::ForecastResult;
pub use identifier::{ModelId, ProviderId};
pub use metric::{Confidence, Metric, MetricValue, Observation, ObservationSource};

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
