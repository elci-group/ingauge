// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use crate::{
    admission::{serve, AdmissionController},
    app::App,
    config::{parse_duration, Config},
    error::AppError,
    telemetry,
};
use chrono::Utc;
use std::path::{Path, PathBuf};
use tokio::time::{interval, MissedTickBehavior};
use tracing::Instrument;

pub async fn run(mut app: App, config_path: Option<PathBuf>) -> Result<(), AppError> {
    let mut period = parse_duration(&app.config.general.poll_interval)
        .map_err(|error| AppError::Configuration(error.to_string()))?;
    let retention = parse_duration(&app.config.general.history_retention)
        .map_err(|error| AppError::Configuration(error.to_string()))?;
    let database_path = app.database_path()?.to_path_buf();
    let mut ticker = interval(period);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_success = None;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut hangup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;

    let admission_controller = AdmissionController::new(app.config.admission.clone());
    let admission_handle = if app.config.admission.enabled {
        let addr =
            app.config.admission.listen_addr.parse().map_err(|error| {
                AppError::Configuration(format!("admission listen_addr: {error}"))
            })?;
        let controller = admission_controller.clone();
        let span = tracing::info_span!("admission_server", address = %addr);
        // traci: allow - the task is explicitly instrumented with the named span below.
        Some(tokio::spawn(async move {
            if let Err(error) = serve(controller, addr).await {
                tracing::error!(event = "admission_server_failed", error = %error, "admission server stopped");
            }
        }
        .instrument(span)))
    } else {
        tracing::info!("admission server disabled");
        None
    };

    tracing::info!(database = %database_path.display(), "daemon started");
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let poll_id = telemetry::poll_correlation_id();
                let span = tracing::info_span!("poll_cycle", poll_id = %poll_id);
                let _entered = span.enter();
                match app.probe().await {
                    Ok(observations) => {
                        last_success = Some(Utc::now());
                        tracing::info!("poll cycle succeeded");
                        let snapshots = app.snapshots(&observations, Utc::now());
                        admission_controller.update_snapshots(snapshots);
                    }
                    Err(error) => tracing::warn!(error_code = error.code(), error = %error, "poll cycle failed"),
                }
                let store = app.open_store()?;
                store.heartbeat(Utc::now(), last_success)?;
                let cutoff = Utc::now() - chrono::Duration::from_std(retention).map_err(|error| AppError::Configuration(error.to_string()))?;
                let pruned = store.prune_before(cutoff)?;
                if pruned > 0 { tracing::info!(event = "history_pruned", pruned, "history retention applied"); }
            }
            result = tokio::signal::ctrl_c() => {
                result?;
                tracing::info!(event = "shutdown_requested", signal = "SIGINT", "interrupt received; shutting down");
                break;
            }
            _ = terminate.recv() => {
                tracing::info!(event = "shutdown_requested", signal = "SIGTERM", "termination signal received; shutting down");
                break;
            }
            _ = hangup.recv() => {
                if let Some(path) = config_path.as_deref() {
                    match reload(path, &database_path) {
                        Ok(config) => {
                            period = parse_duration(&config.general.poll_interval)
                                .map_err(|error| AppError::Configuration(error.to_string()))?;
                            app = App::new(config)?;
                            admission_controller.update_snapshots(Vec::new());
                            ticker = interval(period);
                            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
                            tracing::info!(event = "configuration_reloaded", "configuration reloaded");
                        }
                        Err(error) => tracing::warn!(error = %error, "configuration reload rejected"),
                    }
                }
            }
        }
    }
    if let Some(handle) = admission_handle {
        handle.abort();
    }
    app.open_store()?.checkpoint()?;
    tracing::info!(event = "daemon_stopped", "daemon stopped cleanly");
    Ok(())
}

fn reload(path: &Path, database_path: &Path) -> Result<Config, AppError> {
    let config =
        Config::load(Some(path)).map_err(|error| AppError::Configuration(error.to_string()))?;
    config
        .validate()
        .map_err(|error| AppError::Configuration(error.to_string()))?;
    if config.general.database.as_deref() != Some(database_path) {
        tracing::warn!(
            event = "configuration_reload_rejected",
            reason = "database_path_changed",
            "configuration reload rejected"
        );
        return Err(AppError::Configuration(
            "database path changes require a daemon restart".into(),
        ));
    }
    Ok(config)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn reload_accepts_safe_changes_and_rejects_database_changes() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("ingauge.toml");
        let database = directory.path().join("usage.db");
        std::fs::write(
            &config_path,
            format!(
                "schema_version = 1\n[general]\npoll_interval = \"2s\"\ndatabase = {:?}\n",
                database
            ),
        )
        .unwrap();
        let loaded = reload(&config_path, &database).unwrap();
        assert_eq!(loaded.general.poll_interval, "2s");

        std::fs::write(
            &config_path,
            "schema_version = 1\n[general]\ndatabase = \"other.db\"\n",
        )
        .unwrap();
        assert!(reload(&config_path, &database).is_err());
    }
}
