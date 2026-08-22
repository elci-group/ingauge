// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use super::{AdmissionController, AdmissionDecision, AdmissionRequest};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;

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
