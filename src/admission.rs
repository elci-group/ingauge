// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use crate::{
    config::AdmissionConfig,
    model::{CapacitySnapshot, CapacityState, ModelId, ProviderId},
};
use chrono::{DateTime, Duration, Utc};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};
use tracing;

mod http;
mod protocol;

pub use http::{router, serve};
pub use protocol::{AdmissionDecision, AdmissionRequest, AdmissionResponse};

fn lock_state(inner: &Mutex<InnerState>) -> MutexGuard<'_, InnerState> {
    match inner.lock() {
        Ok(state) => state,
        Err(poisoned) => {
            tracing::error!(
                event = "admission_mutex_poisoned",
                "recovering poisoned admission state"
            );
            poisoned.into_inner()
        }
    }
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
        let mut state = lock_state(&self.inner);
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
                tracing::warn!(event = "invalid_provider_id", provider = %request.provider, %error, "delaying request with invalid provider");
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
                    tracing::warn!(event = "invalid_model_id", model = %value, %error, "delaying request with invalid model");
                    return delay_response(
                        now,
                        self.config.default_delay_ms,
                        format!("invalid model id: {error}"),
                    );
                }
            },
            None => None,
        };

        let mut state = lock_state(&self.inner);
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
            .and_then(|value| match ModelId::new(value) {
                Ok(model) => Some(model),
                Err(error) => {
                    tracing::warn!(event = "invalid_completion_model_id", model = %value, %error, "completing provider-wide lease");
                    None
                }
            });

        let mut state = lock_state(&self.inner);
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

#[cfg(test)]
mod tests;
