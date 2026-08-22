// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use chrono::Utc;
use clap::{Parser, Subcommand};
use ingauge::{
    app::{App, Envelope},
    config::{parse_duration, Config},
    daemon,
    error::AppError,
};
use serde_json::{json, Value};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "ingauge",
    version,
    about = "📊 Inference-capacity observability and forecasting",
    after_help = "💡 Human output is styled automatically. Use --json for automation, NO_COLOR=1 for plain output, or INGAUGE_NO_ANIMATION=1 for reduced motion."
)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// 📊 Show current capacity and upcoming events.
    Status {
        #[arg(long)]
        refresh: bool,
    },
    /// ☁️ List configured capacity targets.
    Providers,
    /// 🛰️ Discover providers, routers, and harnesses on this system.
    Discover {
        #[arg(long)]
        harness_directory: Option<PathBuf>,
    },
    /// 📡 Probe every enabled target now.
    Probe,
    /// 🗂️ Query timestamped capacity observations.
    History {
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        since: Option<String>,
        #[arg(long, default_value_t = 1_000)]
        limit: usize,
    },
    /// 🔭 Forecast consumption from stored observations.
    Forecast {
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
    },
    /// ⚡ Show the next projected capacity events.
    Next,
    /// 💚 Report daemon heartbeat health.
    Health,
    /// 🔄 Run continuous capacity observation.
    Daemon,
    /// ✅ Validate configuration.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// 🗄️ Maintain the capacity database.
    Db {
        #[command(subcommand)]
        command: DbCommand,
    },
    /// 🕸️ Export capacity data to another system.
    Export {
        #[command(subcommand)]
        command: ExportCommand,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    Validate,
}

#[derive(Subcommand)]
enum DbCommand {
    Migrate,
    Integrity,
    Checkpoint,
    Backup { output: PathBuf },
}

#[derive(Subcommand)]
enum ExportCommand {
    Padagonia {
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        since: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    init_tracing();
    let cli = Cli::parse();
    let json_output = cli.json;
    if let Err(error) = run(cli).await {
        tracing::debug!(event = "command_failed", error_code = error.code(), error = %error, "command failed; presenting recovery guidance");
        let code = error.exit_code();
        if json_output {
            let body = json!({
                "schema_version": ingauge::app::JSON_SCHEMA_VERSION,
                "version": env!("CARGO_PKG_VERSION"),
                "generated_at": Utc::now(),
                "data": null,
                "warnings": [],
                "errors": [error.body()],
            });
            if let Ok(rendered) = serde_json::to_string_pretty(&body) {
                eprintln!("{rendered}");
            }
        } else {
            eprintln!("{}", ingauge::presentation::render_error(&error));
        }
        std::process::exit(i32::from(code));
    }
}

async fn run(cli: Cli) -> Result<(), AppError> {
    let resolved_path = Config::resolve_path(cli.config.as_deref());
    let config = Config::load(cli.config.as_deref())
        .map_err(|error| AppError::Configuration(error.to_string()))?;
    config
        .validate()
        .map_err(|error| AppError::Configuration(error.to_string()))?;
    let app = App::new(config)?;
    let command = cli.command.unwrap_or(Command::Status { refresh: false });
    let (name, value) = match command {
        Command::Status { refresh } => ("status", app.status(refresh).await?),
        Command::Providers => {
            let providers: Vec<Value> = app.config.providers.iter().map(|(name, provider)| json!({
                "name": name, "enabled": provider.enabled.unwrap_or(true), "endpoint": provider.endpoint,
                "credential_env": provider.api_key_env,
            })).collect();
            ("providers", json!(providers))
        }
        Command::Discover { harness_directory } => {
            let directory =
                harness_directory.or_else(ingauge::discovery::default_harness_directory);
            (
                "discover",
                json!(ingauge::discovery::discover(directory.as_deref())),
            )
        }
        Command::Probe => {
            let observations = app.probe().await?;
            let snapshots = app.snapshots(&observations, Utc::now());
            (
                "probe",
                json!({"observations": observations, "snapshots": snapshots}),
            )
        }
        Command::History {
            provider,
            model,
            since,
            limit,
        } => {
            let since = since.as_deref().map(parse_since).transpose()?;
            (
                "history",
                json!(app.history(provider.as_deref(), model.as_deref(), since, limit)?),
            )
        }
        Command::Forecast { provider, model } => (
            "forecast",
            app.forecast(provider.as_deref(), model.as_deref())?,
        ),
        Command::Next => {
            let latest = app.open_store()?.latest()?;
            let snapshots = app.snapshots(&latest, Utc::now());
            ("next", json!(ingauge::forecast::events(&snapshots)))
        }
        Command::Health => {
            let poll = parse_duration(&app.config.general.poll_interval)
                .map_err(|error| AppError::Configuration(error.to_string()))?;
            let health = app.open_store()?.health()?;
            let data = health.map(|health| {
                let age = (Utc::now() - health.heartbeat_at).num_seconds().max(0) as u64;
                json!({"status": if age <= poll.as_secs() * 2 { "healthy" } else { "stale" },
                    "heartbeat_at": health.heartbeat_at, "last_success_at": health.last_success_at, "pid": health.pid, "age_seconds": age})
            }).unwrap_or_else(|| json!({"status":"unknown"}));
            ("health", data)
        }
        Command::Daemon => {
            daemon::run(app, resolved_path).await?;
            return Ok(());
        }
        Command::Config {
            command: ConfigCommand::Validate,
        } => ("config_validate", json!({"valid":true})),
        Command::Db { command } => {
            let mut store = app.open_store()?;
            let data = match command {
                DbCommand::Migrate => {
                    store.migrate()?;
                    json!({"migrated":true})
                }
                DbCommand::Integrity => json!({"result":store.integrity_check()?}),
                DbCommand::Checkpoint => {
                    store.checkpoint()?;
                    json!({"checkpointed":true})
                }
                DbCommand::Backup { output } => {
                    store.backup(&output)?;
                    json!({"backup":output})
                }
            };
            ("db", data)
        }
        Command::Export {
            command: ExportCommand::Padagonia { output, since },
        } => {
            let since = since.as_deref().map(parse_since).transpose()?;
            let count = app.export_padagonia(&output, since)?;
            (
                "export_padagonia",
                json!({"output":output,"observations":count}),
            )
        }
    };
    render(name, value, cli.json)
}

fn parse_since(value: &str) -> Result<chrono::DateTime<Utc>, AppError> {
    let duration =
        parse_duration(value).map_err(|error| AppError::Configuration(error.to_string()))?;
    let duration = chrono::Duration::from_std(duration)
        .map_err(|error| AppError::Configuration(error.to_string()))?;
    Ok(Utc::now() - duration)
}

fn render(command: &'static str, value: Value, json_output: bool) -> Result<(), AppError> {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&Envelope::success(command, value))?
        );
    } else {
        if let Err(error) = ingauge::presentation::animate(command) {
            tracing::debug!(event = "output_animation_skipped", error = %error, "terminal animation unavailable");
        }
        println!("{}", ingauge::presentation::render(command, &value));
    }
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(false)
        .try_init();
}
