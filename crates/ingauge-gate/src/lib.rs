// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
//! Optional client for the ingauge admission gate.
//!
//! This crate wraps LLM calls with a lightweight admission request to a local
//! ingauge daemon.  If ingauge is unreachable or disabled, the gate falls back
//! to letting the call proceed immediately, so ingauge never becomes a hard
//! dependency for the calling application.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use ingauge_gate::Admitter;
//!
//! #[tokio::main]
//! async fn main() {
//!     let admitter = Admitter::from_env();
//!     admitter.wait_and_admit("groq", Some("llama3-70b"), None).await.unwrap();
//!     // ... make the actual LLM call with your own API key ...
//! }
//! ```

mod presentation;

use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use std::{env, time::Duration};

/// Default URL used when `INGAUGE_URL` is not set.
pub const DEFAULT_URL: &str = "http://127.0.0.1:8080";

/// Default timeout for admission requests.
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 5;

/// Admission decision returned by ingauge.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionDecision {
    Proceed,
    Delay,
}

/// Request body for `POST /admit`.
#[derive(Debug, Clone, Serialize)]
pub struct AdmissionRequest {
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
}

/// Response body from `POST /admit`.
#[derive(Debug, Clone, Deserialize)]
pub struct AdmissionResponse {
    pub decision: AdmissionDecision,
    pub delay_ms: u64,
    pub reason: String,
    pub estimated_start_at: DateTime<Utc>,
}

/// Error kinds for the gate client.
#[derive(Debug, thiserror::Error)]
pub enum GateError {
    #[error("admission request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("invalid ingauge url: {0}")]
    InvalidUrl(String),
    #[error("admission endpoint returned {status}: {body}")]
    Endpoint { status: StatusCode, body: String },
}

/// Configuration for the gate client.
#[derive(Debug, Clone)]
pub struct Admitter {
    client: Client,
    base_url: Url,
    fallback_on_error: bool,
    show_timer: bool,
}

impl Admitter {
    /// Build an admitter from explicit parameters.
    ///
    /// Set `fallback_on_error` to `true` so that a missing or unhealthy ingauge
    /// daemon does not block the calling tool.
    pub fn new(
        base_url: impl AsRef<str>,
        timeout: Duration,
        fallback_on_error: bool,
        show_timer: bool,
    ) -> Result<Self, GateError> {
        let base_url = base_url
            .as_ref()
            .parse::<Url>()
            .map_err(|e| GateError::InvalidUrl(e.to_string()))?;
        let client = Client::builder()
            .timeout(timeout)
            .user_agent(concat!("ingauge-gate/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(GateError::Request)?;
        Ok(Self {
            client,
            base_url,
            fallback_on_error,
            show_timer,
        })
    }

    /// Build an admitter from environment variables.
    ///
    /// Reads:
    /// - `INGAUGE_URL` (default: `http://127.0.0.1:8080`)
    /// - `INGAUGE_ADMIT_TIMEOUT` seconds (default: 5)
    /// - `INGAUGE_FALLBACK` (default: `true`)
    /// - `INGAUGE_NO_TIMER` (default: `false`)
    /// - `INGAUGE_NO_ANIMATION` (default: `false`)
    /// - `NO_COLOR` (unset by default)
    pub fn from_env() -> Self {
        let base_url = std::env::var("INGAUGE_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
        let timeout = std::env::var("INGAUGE_ADMIT_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(DEFAULT_TIMEOUT_SECONDS));
        let fallback_on_error = std::env::var("INGAUGE_FALLBACK")
            .map(|v| !v.eq_ignore_ascii_case("false") && v != "0")
            .unwrap_or(true);
        let show_timer = std::env::var("INGAUGE_NO_TIMER")
            .map(|v| v.eq_ignore_ascii_case("false") || v == "0")
            .unwrap_or(true);

        Self::new(base_url, timeout, fallback_on_error, show_timer).unwrap_or_else(|error| {
            tracing::warn!(%error, "failed to build admitter from env; using fallback");
            Self {
                client: Client::new(),
                base_url: DEFAULT_URL.parse().unwrap(),
                fallback_on_error: true,
                show_timer,
            }
        })
    }

    /// Ask ingauge whether a call may proceed.
    pub async fn admit(
        &self,
        provider: impl Into<String>,
        model: Option<impl Into<String>>,
        estimated_tokens: Option<u64>,
    ) -> Result<AdmissionResponse, GateError> {
        let request = AdmissionRequest {
            provider: provider.into(),
            model: model.map(Into::into),
            estimated_input_tokens: estimated_tokens,
            estimated_output_tokens: None,
            priority: None,
        };
        let url = self
            .base_url
            .join("/admit")
            .map_err(|e| GateError::InvalidUrl(e.to_string()))?;
        let response = self.client.post(url).json(&request).send().await?;
        let status = response.status();
        if !status.is_success() && status != StatusCode::TOO_MANY_REQUESTS {
            let body = response.text().await.unwrap_or_default();
            return Err(GateError::Endpoint { status, body });
        }
        Ok(response.json::<AdmissionResponse>().await?)
    }

    /// Ask for admission and block until the call may start.
    ///
    /// When ingauge returns `Delay`, this prints a live cooldown timer to
    /// stderr (unless disabled), sleeps until the estimated start time, and
    /// re-admits in case capacity changed while waiting.
    pub async fn wait_and_admit(
        &self,
        provider: impl Into<String>,
        model: Option<impl Into<String>>,
        estimated_tokens: Option<u64>,
    ) -> Result<(), GateError> {
        let provider = provider.into();
        let model = model.map(Into::into);

        loop {
            match self.admit(&provider, model.clone(), estimated_tokens).await {
                Ok(response) => {
                    if response.decision == AdmissionDecision::Proceed {
                        return Ok(());
                    }
                    self.wait_once(&provider, model.as_deref(), &response).await;
                    // Loop around and re-admit.
                }
                Err(error) if self.fallback_on_error => {
                    tracing::warn!(%error, "ingauge admission failed; proceeding without gating");
                    return Ok(());
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Report that a previously admitted LLM call has completed.
    ///
    /// Best-effort: failures are ignored so that a stale `/complete` cannot
    /// break the caller.
    pub async fn complete(&self, provider: impl Into<String>, model: Option<impl Into<String>>) {
        let request = AdmissionRequest {
            provider: provider.into(),
            model: model.map(Into::into),
            estimated_input_tokens: None,
            estimated_output_tokens: None,
            priority: None,
        };
        let url = match self.base_url.join("/complete") {
            Ok(url) => url,
            Err(error) => {
                tracing::debug!(%error, "failed to build /complete url");
                return;
            }
        };
        if let Err(error) = self.client.post(url).json(&request).send().await {
            tracing::debug!(%error, "ingauge /complete failed; lease will expire naturally");
        }
    }

    async fn wait_once(&self, provider: &str, model: Option<&str>, response: &AdmissionResponse) {
        let target = model
            .map(|m| format!("{provider}/{m}"))
            .unwrap_or_else(|| provider.to_string());
        let wait_until = response.estimated_start_at;
        let now = Utc::now();
        let wait_duration = if wait_until > now {
            (wait_until - now)
                .to_std()
                .unwrap_or(Duration::from_millis(response.delay_ms))
        } else {
            Duration::from_millis(response.delay_ms)
        };

        presentation::wait(&target, &response.reason, wait_duration, self.show_timer).await;
    }
}

impl Default for Admitter {
    fn default() -> Self {
        Self::from_env()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn proceed_returns_immediately() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admit"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "decision": "proceed",
                "delay_ms": 0,
                "reason": "ok",
                "estimated_start_at": Utc::now().to_rfc3339()
            })))
            .mount(&server)
            .await;

        let admitter = Admitter::new(server.uri(), Duration::from_secs(5), false, false).unwrap();
        admitter
            .wait_and_admit("groq", Some("m"), None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn missing_ingauge_falls_back_when_configured() {
        let admitter = Admitter::new(
            "http://127.0.0.1:1",
            Duration::from_millis(100),
            true,
            false,
        )
        .unwrap();
        admitter
            .wait_and_admit("groq", Some("m"), None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn missing_ingauge_errors_when_fallback_disabled() {
        let admitter = Admitter::new(
            "http://127.0.0.1:1",
            Duration::from_millis(100),
            false,
            false,
        )
        .unwrap();
        assert!(admitter
            .wait_and_admit("groq", Some("m"), None)
            .await
            .is_err());
    }
}
