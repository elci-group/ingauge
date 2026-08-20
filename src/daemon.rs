use crate::{
    app::App,
    config::{parse_duration, Config},
    error::AppError,
};
use chrono::Utc;
use std::path::{Path, PathBuf};
use tokio::time::{interval, MissedTickBehavior};

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

    tracing::info!(database = %database_path.display(), "daemon started");
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let poll_id = format!("{}-{}", std::process::id(), Utc::now().timestamp_millis());
                let span = tracing::info_span!("poll_cycle", poll_id = %poll_id);
                let _entered = span.enter();
                match app.probe().await {
                    Ok(_) => { last_success = Some(Utc::now()); tracing::info!("poll cycle succeeded"); }
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
