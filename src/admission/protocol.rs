use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
