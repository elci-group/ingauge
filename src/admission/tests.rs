use super::*;
use crate::model::{Confidence, Quota};

fn test_config() -> AdmissionConfig {
    AdmissionConfig {
        enabled: true,
        listen_addr: "127.0.0.1:0".into(),
        max_concurrent: 2,
        default_delay_ms: 1_000,
        admit_when_unknown: true,
    }
}

fn snapshot_with_state(state: CapacityState) -> CapacitySnapshot {
    CapacitySnapshot {
        provider: ProviderId::new("groq").expect("valid provider id"),
        model: Some(ModelId::new("llama3-70b").expect("valid model id")),
        available: state != CapacityState::Exhausted,
        remaining: vec![Quota {
            metric: crate::model::Metric::RemainingRequests,
            used: Some(0),
            limit: Some(100),
            remaining: Some(100),
            reset_at: None,
        }],
        utilisation: vec![],
        consumption_rate: None,
        next_reset: None,
        exhaustion: None,
        headroom: None,
        state,
        confidence: Confidence::High,
        observed_at: Utc::now(),
        error: None,
    }
}

#[test]
fn admits_when_healthy() {
    let controller = AdmissionController::new(test_config());
    controller.update_snapshots(vec![snapshot_with_state(CapacityState::Healthy)]);
    let response = controller.admit(&AdmissionRequest {
        provider: "groq".into(),
        model: Some("llama3-70b".into()),
        estimated_input_tokens: None,
        estimated_output_tokens: None,
        priority: None,
    });
    assert_eq!(response.decision, AdmissionDecision::Proceed);
}

#[test]
fn delays_when_critical() {
    let controller = AdmissionController::new(test_config());
    controller.update_snapshots(vec![snapshot_with_state(CapacityState::Critical)]);
    let response = controller.admit(&AdmissionRequest {
        provider: "groq".into(),
        model: Some("llama3-70b".into()),
        estimated_input_tokens: None,
        estimated_output_tokens: None,
        priority: None,
    });
    assert_eq!(response.decision, AdmissionDecision::Delay);
    assert!(response.delay_ms > 0);
}

#[test]
fn enforces_concurrency_limit() {
    let controller = AdmissionController::new(test_config());
    controller.update_snapshots(vec![snapshot_with_state(CapacityState::Healthy)]);
    let request = AdmissionRequest {
        provider: "groq".into(),
        model: Some("llama3-70b".into()),
        estimated_input_tokens: None,
        estimated_output_tokens: None,
        priority: None,
    };
    assert_eq!(
        controller.admit(&request).decision,
        AdmissionDecision::Proceed
    );
    assert_eq!(
        controller.admit(&request).decision,
        AdmissionDecision::Proceed
    );
    assert_eq!(
        controller.admit(&request).decision,
        AdmissionDecision::Delay
    );
}

#[test]
fn falls_back_to_provider_wide_snapshot() {
    let controller = AdmissionController::new(test_config());
    let mut snapshot = snapshot_with_state(CapacityState::Healthy);
    snapshot.model = None;
    controller.update_snapshots(vec![snapshot]);
    let response = controller.admit(&AdmissionRequest {
        provider: "groq".into(),
        model: Some("some-model".into()),
        estimated_input_tokens: None,
        estimated_output_tokens: None,
        priority: None,
    });
    assert_eq!(response.decision, AdmissionDecision::Proceed);
}
