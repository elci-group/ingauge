use crate::{
    config::AdmissionConfig,
    model::{CapacitySnapshot, CapacityState, ModelId, ProviderId},
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;
use tracing;

/// Request body for `POST /admit`.
#[derive(Debug, Clone, Deserialize)]
pub struct AdmissionRequest {
    pub provider: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub estimated_input_tokens: Option<u64>,
    #[serde(default)]
    pub estimated_output_tokens: Option<u64>,
    #[serde(default)]
    pub priority: Option<u8>,
}

/// Decision variants returned by the admission controller.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionDecision {
    Proceed,
    Delay,
}

/// Response body for `POST /admit`.
#[derive(Debug, Clone, Serialize)]
pub struct AdmissionResponse {
    pub decision: AdmissionDecision,
    pub delay_ms: u64,
    pub reason: String,
    pub estimated_start_at: DateTime<Utc>,
}

/// State kept for a single in-flight admission lease.
#[derive(Debug, Clone)]
struct Lease {
    expires_at: DateTime<Utc>,
}

/// In-memory admission state, protected by a mutex because the HTTP
/// handlers and the expiry reaper share it.
#[derive(Debug, Default)]
struct InnerState {
    snapshots: HashMap<(ProviderId, Option<ModelId>), CapacitySnapshot>,
    leases: HashMap<(ProviderId, Option<ModelId>), Vec<Lease>>,
}

/// Shared admission controller used by the daemon and the HTTP server.
#[derive(Clone)]
pub struct AdmissionController {
    config: AdmissionConfig,
    inner: Arc<Mutex<InnerState>>,
}

impl AdmissionController {
    pub fn new(config: AdmissionConfig) -> Self {
        Self {
            config,
            inner: Arc::new(Mutex::new(InnerState::default())),
        }
    }

    /// Update the capacity snapshots used for admission decisions.
    pub fn update_snapshots(&self, snapshots: Vec<CapacitySnapshot>) {
        let mut state = self.inner.lock().expect("admission mutex poisoned");
        state.snapshots.clear();
        for snapshot in snapshots {
            state.snapshots.insert(
                (snapshot.provider.clone(), snapshot.model.clone()),
                snapshot,
            );
        }
        tracing::debug!(
            snapshots = state.snapshots.len(),
            "admission snapshots updated"
        );
    }

    /// Decide whether a request may proceed now or must wait.
    pub fn admit(&self, request: &AdmissionRequest) -> AdmissionResponse {
        let now = Utc::now();
        let provider = match ProviderId::new(&request.provider) {
            Ok(id) => id,
            Err(error) => {
                return delay_response(
                    now,
                    self.config.default_delay_ms,
                    format!("invalid provider id: {error}"),
                );
            }
        };
        let model = match request.model.as_deref() {
            Some(value) => match ModelId::new(value) {
                Ok(id) => Some(id),
                Err(error) => {
                    return delay_response(
                        now,
                        self.config.default_delay_ms,
                        format!("invalid model id: {error}"),
                    );
                }
            },
            None => None,
        };

        let mut state = self.inner.lock().expect("admission mutex poisoned");
        self.prune_expired(&mut state, now);

        let key = (provider.clone(), model.clone());
        let snapshot = state.snapshots.get(&key).or_else(|| {
            // Fall back to provider-wide snapshot if a per-model snapshot is unavailable.
            state.snapshots.get(&(provider.clone(), None))
        });

        let in_flight = state.leases.get(&key).map(Vec::len).unwrap_or(0);

        if in_flight >= self.config.max_concurrent {
            return delay_response(
                now,
                self.config.default_delay_ms,
                format!(
                    "concurrency limit reached for {provider}/{}",
                    model.as_ref().map(ModelId::as_str).unwrap_or("_")
                ),
            );
        }

        let decision = match snapshot {
            Some(snapshot) => match snapshot.state {
                CapacityState::Healthy | CapacityState::Recovering | CapacityState::Moderate => {
                    DecisionOutcome::Proceed
                }
                CapacityState::Constrained | CapacityState::Critical | CapacityState::Exhausted => {
                    let delay_ms = estimate_recovery_delay(snapshot, &self.config);
                    DecisionOutcome::DelayWithReason(
                        format!(
                            "capacity {} for {provider}/{}",
                            format_state(snapshot.state),
                            model.as_ref().map(ModelId::as_str).unwrap_or("_")
                        ),
                        delay_ms,
                    )
                }
                CapacityState::Unknown => {
                    if self.config.admit_when_unknown {
                        DecisionOutcome::Proceed
                    } else {
                        DecisionOutcome::DelayWithReason(
                            "capacity unknown".into(),
                            self.config.default_delay_ms,
                        )
                    }
                }
            },
            None => {
                if self.config.admit_when_unknown {
                    DecisionOutcome::Proceed
                } else {
                    DecisionOutcome::DelayWithReason(
                        "no capacity snapshot available".into(),
                        self.config.default_delay_ms,
                    )
                }
            }
        };

        match decision {
            DecisionOutcome::Proceed => {
                let lease_ttl = estimate_lease_ttl(request);
                state.leases.entry(key).or_default().push(Lease {
                    expires_at: now + lease_ttl,
                });
                AdmissionResponse {
                    decision: AdmissionDecision::Proceed,
                    delay_ms: 0,
                    reason: "admitted".into(),
                    estimated_start_at: now,
                }
            }
            DecisionOutcome::DelayWithReason(reason, delay_ms) => {
                delay_response(now, delay_ms, reason)
            }
        }
    }

    /// Explicitly release a previously granted admission lease.
    ///
    /// Callers that can report completion should POST to `/complete` with the
    /// same provider/model so the concurrency budget frees immediately.
    pub fn complete(&self, request: &AdmissionRequest) -> AdmissionResponse {
        let now = Utc::now();
        let provider = match ProviderId::new(&request.provider) {
            Ok(id) => id,
            Err(_) => return proceed_response(now, "invalid provider, nothing to complete".into()),
        };
        let model = request
            .model
            .as_deref()
            .and_then(|value| ModelId::new(value).ok());

        let mut state = self.inner.lock().expect("admission mutex poisoned");
        let key = (provider.clone(), model.clone());
        if let Some(leases) = state.leases.get_mut(&key) {
            // Remove the oldest still-valid lease as a best-effort completion signal.
            leases.retain(|lease| lease.expires_at > now);
            if !leases.is_empty() {
                leases.remove(0);
            }
        }
        proceed_response(now, "completed".into())
    }

    fn prune_expired(&self, state: &mut InnerState, now: DateTime<Utc>) {
        for leases in state.leases.values_mut() {
            leases.retain(|lease| lease.expires_at > now);
        }
        state.leases.retain(|_, leases| !leases.is_empty());
    }
}

enum DecisionOutcome {
    Proceed,
    DelayWithReason(String, u64),
}

fn delay_response(now: DateTime<Utc>, delay_ms: u64, reason: String) -> AdmissionResponse {
    AdmissionResponse {
        decision: AdmissionDecision::Delay,
        delay_ms,
        reason,
        estimated_start_at: now + Duration::milliseconds(delay_ms as i64),
    }
}

fn proceed_response(now: DateTime<Utc>, reason: String) -> AdmissionResponse {
    AdmissionResponse {
        decision: AdmissionDecision::Proceed,
        delay_ms: 0,
        reason,
        estimated_start_at: now,
    }
}

fn format_state(state: CapacityState) -> &'static str {
    match state {
        CapacityState::Healthy => "healthy",
        CapacityState::Recovering => "recovering",
        CapacityState::Moderate => "moderate",
        CapacityState::Constrained => "constrained",
        CapacityState::Critical => "critical",
        CapacityState::Exhausted => "exhausted",
        CapacityState::Unknown => "unknown",
    }
}

fn estimate_recovery_delay(snapshot: &CapacitySnapshot, config: &AdmissionConfig) -> u64 {
    // Prefer the next quota reset if known.
    if let Some(reset_at) = snapshot.next_reset {
        let ms = (reset_at - Utc::now()).num_milliseconds().max(0) as u64;
        if ms > 0 {
            return ms;
        }
    }
    // Otherwise use a multiple of the default delay based on severity.
    match snapshot.state {
        CapacityState::Exhausted => config.default_delay_ms.saturating_mul(4),
        CapacityState::Critical => config.default_delay_ms.saturating_mul(2),
        _ => config.default_delay_ms,
    }
}

fn estimate_lease_ttl(request: &AdmissionRequest) -> Duration {
    // Rough heuristic: ~1s per 1000 estimated tokens, minimum 5s, maximum 120s.
    let tokens = request
        .estimated_input_tokens
        .unwrap_or(0)
        .saturating_add(request.estimated_output_tokens.unwrap_or(0));
    let seconds = (tokens / 1000).clamp(5, 120);
    Duration::seconds(seconds as i64)
}

async fn handle_admit(
    State(controller): State<AdmissionController>,
    Json(request): Json<AdmissionRequest>,
) -> impl IntoResponse {
    let response = controller.admit(&request);
    let status = if response.decision == AdmissionDecision::Proceed {
        StatusCode::OK
    } else {
        StatusCode::TOO_MANY_REQUESTS
    };
    (status, Json(response))
}

async fn handle_complete(
    State(controller): State<AdmissionController>,
    Json(request): Json<AdmissionRequest>,
) -> impl IntoResponse {
    let response = controller.complete(&request);
    (StatusCode::OK, Json(response))
}

/// Build an axum router for the admission API.
pub fn router(controller: AdmissionController) -> Router {
    Router::new()
        .route("/admit", post(handle_admit))
        .route("/complete", post(handle_complete))
        .with_state(controller)
}

/// Bind the admission HTTP server and return its future.
///
/// The returned future should be spawned onto the tokio runtime; it resolves
/// only on error or shutdown.
pub async fn serve(controller: AdmissionController, addr: SocketAddr) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(address = %addr, "admission server listening");
    axum::serve(listener, router(controller)).await
}

#[cfg(test)]
mod tests {
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
}
