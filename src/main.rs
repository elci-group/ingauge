mod curly_expand;
// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use chrono::Utc;
use clap::{Parser, Subcommand};
use ingauge::{
    app::{App, Envelope},
    config::{parse_duration, Config},
    daemon,
    error::AppError,
    instrument::{GaugeConfig, NeedleState},
    network::{NetworkActivity, NetworkMonitor},
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, IsTerminal, Write as _},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
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
    /// 🏁 Open the interactive provider configuration garage.
    Tui,
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
async fn __curly_original_main() {
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
    let command = cli.command.unwrap_or(Command::Status { refresh: false });
    let opens_tui = matches!(
        &command,
        Command::Config {
            command: ConfigCommand::Tui
        }
    );
    let interactive_status = matches!(&command, Command::Status { .. })
        && !cli.json
        && io::stdin().is_terminal()
        && io::stdout().is_terminal();
    let may_create_config = opens_tui || interactive_status;
    let mut config =
        if may_create_config && resolved_path.as_deref().is_some_and(|path| !path.exists()) {
            Config::default()
        } else {
            Config::load(cli.config.as_deref())
                .map_err(|error| AppError::Configuration(error.to_string()))?
        };
    let has_provider = config
        .providers
        .values()
        .any(|provider| provider.enabled.unwrap_or(true));
    let automatic_tui = should_open_setup(
        has_provider,
        matches!(&command, Command::Status { .. }),
        cli.json,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    );
    if opens_tui || automatic_tui {
        if cli.json {
            return Err(AppError::Configuration(
                "config tui is interactive and cannot be combined with --json".into(),
            ));
        }
        let path = resolved_path.unwrap_or_else(|| PathBuf::from("ingauge.toml"));
        let result = ingauge::setup::run(&mut config, &path)
            .map_err(|error| AppError::Configuration(error.to_string()))?;
        return render("config_tui", json!(result), false);
    }
    config
        .validate()
        .map_err(|error| AppError::Configuration(error.to_string()))?;
    let app = App::new(config)?;
    let (name, value) = match command {
        Command::Status { refresh } => {
            let status = app.status(refresh || interactive_status).await?;
            if interactive_status {
                live_dashboard(&app, status).await?;
                return Ok(());
            }
            ("status", status)
        }
        Command::Providers => {
            let providers: Vec<Value> = app.config.providers.iter().map(|(name, provider)| json!({
                "name": name, "enabled": provider.enabled.unwrap_or(true), "endpoint": provider.endpoint,
                "credential_env": provider.api_key_env,
                "credential_source": provider.credential_source,
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
        Command::Config {
            command: ConfigCommand::Tui,
        } => unreachable!("config tui is handled before application startup"),
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

fn should_open_setup(
    has_provider: bool,
    status_command: bool,
    json_output: bool,
    stdin_terminal: bool,
    stdout_terminal: bool,
) -> bool {
    !has_provider && status_command && !json_output && stdin_terminal && stdout_terminal
}

fn provider_names(value: &Value) -> Vec<String> {
    let mut providers = value["snapshots"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|snapshot| snapshot["provider"].as_str())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    providers.extend(
        value["configured_providers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    providers.into_iter().collect()
}

#[derive(Default)]
struct AnimatedInstrumentSet {
    rpm: NeedleState,
    tpm: NeedleState,
    rpd: NeedleState,
    initialized: bool,
}

struct DashboardAnimator {
    instruments: BTreeMap<String, AnimatedInstrumentSet>,
    started: Instant,
    previous_frame: Instant,
    reduced_motion: bool,
}

impl DashboardAnimator {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            instruments: BTreeMap::new(),
            started: now,
            previous_frame: now,
            reduced_motion: std::env::var_os("INGAUGE_NO_ANIMATION").is_some(),
        }
    }

    fn advance(&mut self, value: &Value) -> Value {
        let now = Instant::now();
        let dt = now.duration_since(self.previous_frame).as_secs_f64();
        self.previous_frame = now;
        let elapsed = now.duration_since(self.started).as_secs_f64();
        let mut frame = value.clone();
        let rpm_max = frame["instruments"]["rpm"]["max"]
            .as_f64()
            .unwrap_or(10_000.0);
        let rpm_redline = frame["instruments"]["rpm"]["redline"]
            .as_f64()
            .unwrap_or(rpm_max * 0.85);
        let tpm_max = frame["instruments"]["tpm"]["max"]
            .as_f64()
            .unwrap_or(100_000.0);
        let tpm_redline = frame["instruments"]["tpm"]["redline"]
            .as_f64()
            .unwrap_or(tpm_max * 0.85);
        let rpd_limit = frame["instruments"]["rpd"]["daily_limit"]
            .as_f64()
            .unwrap_or(100_000.0);
        let rpm_gauge = GaugeConfig::tachometer(rpm_max, rpm_redline);
        let tpm_gauge = GaugeConfig::speedometer(tpm_max, tpm_redline);
        let rpd_gauge = GaugeConfig::oil(rpd_limit, rpd_limit * 0.75, rpd_limit * 0.9);
        if let Some(snapshots) = frame["snapshots"].as_array_mut() {
            for snapshot in snapshots {
                let key = format!(
                    "{}:{}",
                    snapshot["provider"].as_str().unwrap_or("unknown"),
                    snapshot["model"].as_str().unwrap_or("provider-wide")
                );
                let needles = self.instruments.entry(key).or_default();
                if !needles.initialized {
                    needles.rpm.current_angle = rpm_gauge.start_angle;
                    needles.tpm.current_angle = tpm_gauge.start_angle;
                    needles.rpd.current_angle = rpd_gauge.start_angle;
                    needles.initialized = true;
                }
                let startup_target = if self.reduced_motion {
                    None
                } else if elapsed < 0.55 {
                    Some(1.0)
                } else if elapsed < 1.1 {
                    Some(0.0)
                } else {
                    None
                };
                let rpm = startup_target.map_or_else(
                    || snapshot["telemetry"]["rpm"].as_f64().unwrap_or(0.0),
                    |ratio| rpm_max * ratio,
                );
                let tpm = startup_target.map_or_else(
                    || snapshot["telemetry"]["tpm"].as_f64().unwrap_or(0.0),
                    |ratio| tpm_max * ratio,
                );
                let rpd = snapshot["telemetry"]["rpd"].as_f64().unwrap_or(0.0);
                if self.reduced_motion {
                    needles.rpm.current_angle = rpm_gauge.value_to_angle(rpm);
                    needles.tpm.current_angle = tpm_gauge.value_to_angle(tpm);
                    needles.rpd.current_angle = rpd_gauge.value_to_angle(rpd);
                    needles.rpm.velocity = 0.0;
                    needles.tpm.velocity = 0.0;
                    needles.rpd.velocity = 0.0;
                } else {
                    advance_needle(&mut needles.rpm, rpm_gauge.value_to_angle(rpm), dt, 14.0);
                    advance_needle(&mut needles.tpm, tpm_gauge.value_to_angle(tpm), dt, 17.0);
                    advance_needle(&mut needles.rpd, rpd_gauge.value_to_angle(rpd), dt, 9.0);
                }
                snapshot["telemetry"]["rpm"] =
                    json!(rpm_gauge.angle_to_value(needles.rpm.current_angle));
                snapshot["telemetry"]["tpm"] =
                    json!(tpm_gauge.angle_to_value(needles.tpm.current_angle));
                snapshot["telemetry"]["rpd"] =
                    json!(rpd_gauge.angle_to_value(needles.rpd.current_angle));
            }
        }
        frame["live"]["phase"] = json!(if !self.reduced_motion && elapsed < 1.1 {
            "IGNITION"
        } else {
            "LIVE"
        });
        frame["live"]["reduced_motion"] = json!(self.reduced_motion);
        frame
    }
}

fn advance_needle(needle: &mut NeedleState, target: f64, dt: f64, responsiveness: f64) {
    let steps = (dt / 0.016).ceil().clamp(1.0, 8.0) as usize;
    let substep = dt / steps as f64;
    for _ in 0..steps {
        needle.advance(target, substep, responsiveness);
    }
}

fn provider_frame(value: &Value, position: usize, interval_seconds: u64) -> Value {
    let providers = provider_names(value);
    if providers.is_empty() {
        return value.clone();
    }
    let position = position % providers.len();
    let provider = &providers[position];
    let snapshots = value["snapshots"].as_array().cloned().unwrap_or_default();
    let mut frame = value.clone();
    frame["snapshots"] = Value::Array(
        snapshots
            .into_iter()
            .filter(|snapshot| snapshot["provider"].as_str() == Some(provider.as_str()))
            .collect(),
    );
    frame["configured_providers"] = json!([provider]);
    frame["cycle"] = json!({
        "position": position + 1,
        "total": providers.len(),
        "interval_seconds": interval_seconds,
    });
    frame
}

fn apply_network_activity(value: &mut Value, activity: &BTreeMap<String, NetworkActivity>) {
    for (provider, reading) in activity {
        value["network_activity"][provider] = json!(reading);
        let mut matched = false;
        if let Some(snapshots) = value["snapshots"].as_array_mut() {
            for snapshot in snapshots
                .iter_mut()
                .filter(|snapshot| snapshot["provider"].as_str() == Some(provider.as_str()))
            {
                matched = true;
                let rpm_missing = snapshot["telemetry"]["rpm"]
                    .as_f64()
                    .is_none_or(|value| value <= 0.0);
                let tpm_missing = snapshot["telemetry"]["tpm"]
                    .as_f64()
                    .is_none_or(|value| value <= 0.0);
                if rpm_missing {
                    snapshot["telemetry"]["rpm"] = json!(reading.requests_per_minute);
                }
                if tpm_missing {
                    snapshot["telemetry"]["tpm"] = json!(reading.estimated_tokens_per_minute);
                }
                if rpm_missing || tpm_missing {
                    snapshot["network_estimated"] = json!(true);
                    snapshot["network"] = json!(reading);
                }
            }
            if !matched
                && (reading.active_connections > 0
                    || reading.detected_calls > 0
                    || reading.estimated_tokens_per_minute > 0.0)
            {
                snapshots.push(json!({
                    "provider": provider,
                    "model": "encrypted network traffic",
                    "state": if reading.requests_per_minute > 0.0 { "accelerating" } else { "idle" },
                    "freshness": "live",
                    "confidence": "estimated",
                    "network_estimated": true,
                    "network": reading,
                    "telemetry": {
                        "rpm": reading.requests_per_minute,
                        "tpm": reading.estimated_tokens_per_minute,
                        "rpd": 0.0
                    }
                }));
            }
        }
    }
}

async fn live_dashboard(app: &App, initial: Value) -> Result<(), AppError> {
    let sample_seconds = app.config.instruments.dashboard_sample_seconds;
    let cycle_seconds = app.config.instruments.provider_cycle_seconds;
    let latest = Arc::new(Mutex::new(initial));
    let sampler_value = Arc::clone(&latest);
    let sampler_app = app.clone();
    let _sampler = AbortTask(tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(sample_seconds));
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Ok(status) = sampler_app.status(true).await {
                if let Ok(mut value) = sampler_value.lock() {
                    *value = status;
                }
            }
        }
    }));
    let network_activity = Arc::new(Mutex::new(BTreeMap::<String, NetworkActivity>::new()));
    let _network_sampler = if app.config.instruments.network.enabled {
        let network_value = Arc::clone(&network_activity);
        let interval_ms = app.config.instruments.network.sample_interval_ms;
        let mut monitor = NetworkMonitor::from_config(&app.config);
        Some(AbortTask(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms));
            loop {
                ticker.tick().await;
                match monitor.sample() {
                    Ok(activity) => {
                        if let Ok(mut value) = network_value.lock() {
                            *value = activity;
                        }
                    }
                    Err(error) => {
                        tracing::debug!(event = "network_monitor_sample_failed", %error, "network telemetry unavailable");
                    }
                }
            }
        })))
    } else {
        None
    };
    let started = Instant::now();
    let mut animator = DashboardAnimator::new();
    let _terminal = TerminalSession::enter()?;
    let mut stdout = io::stdout().lock();
    loop {
        let mut value = latest
            .lock()
            .map_err(|_| AppError::Io(io::Error::other("dashboard state lock poisoned")))?
            .clone();
        let activity = network_activity
            .lock()
            .map_err(|_| AppError::Io(io::Error::other("network state lock poisoned")))?
            .clone();
        apply_network_activity(&mut value, &activity);
        let elapsed = started.elapsed();
        let position = (elapsed.as_secs() / cycle_seconds) as usize;
        let mut frame = provider_frame(&value, position, cycle_seconds);
        frame["live"]["sample_seconds"] = json!(sample_seconds);
        frame["live"]["elapsed_ms"] = json!(elapsed.as_millis() as u64);
        let frame = animator.advance(&frame);
        write!(
            stdout,
            "\x1b[2J\x1b[H{}",
            ingauge::presentation::render("status", &frame)
        )?;
        stdout.flush()?;
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(80)) => {}
            signal = tokio::signal::ctrl_c() => {
                signal?;
                return Ok(());
            }
        }
    }
}

struct TerminalSession;

struct AbortTask(tokio::task::JoinHandle<()>);

impl Drop for AbortTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        let mut stdout = io::stdout().lock();
        write!(stdout, "\x1b[?1049h\x1b[?25l")?;
        stdout.flush()?;
        Ok(Self)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let mut stdout = io::stdout().lock();
        let _ = write!(stdout, "\x1b[?25h\x1b[?1049l");
        let _ = stdout.flush();
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_setup_is_limited_to_an_interactive_unconfigured_status() {
        assert!(should_open_setup(false, true, false, true, true));
        assert!(!should_open_setup(true, true, false, true, true));
        assert!(!should_open_setup(false, false, false, true, true));
        assert!(!should_open_setup(false, true, true, true, true));
        assert!(!should_open_setup(false, true, false, false, true));
        assert!(!should_open_setup(false, true, false, true, false));
    }

    #[test]
    fn provider_rotation_groups_models_by_provider() {
        let value = json!({"snapshots": [
            {"provider": "groq", "model": "a"},
            {"provider": "openai", "model": "b"},
            {"provider": "groq", "model": "c"}
        ], "configured_providers": ["anthropic", "groq", "openai"]});
        assert_eq!(provider_names(&value), vec!["anthropic", "groq", "openai"]);
        let frame = provider_frame(&value, 1, 4);
        assert_eq!(frame["configured_providers"], json!(["groq"]));
        assert_eq!(frame["snapshots"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn dashboard_needles_advance_between_render_frames() {
        let value = json!({
            "instruments": {
                "rpm": {"max": 10000.0, "redline": 8500.0},
                "tpm": {"max": 100000.0, "redline": 85000.0},
                "rpd": {"daily_limit": 100000.0}
            },
            "snapshots": [{
                "provider": "groq",
                "model": null,
                "telemetry": {"rpm": 5000.0, "tpm": 50000.0, "rpd": 50000.0}
            }]
        });
        let mut animator = DashboardAnimator::new();
        animator.started = Instant::now() - Duration::from_secs(2);
        animator.previous_frame = Instant::now() - Duration::from_millis(80);
        let first = animator.advance(&value);
        animator.previous_frame = Instant::now() - Duration::from_millis(80);
        let second = animator.advance(&value);
        let first_tpm = first["snapshots"][0]["telemetry"]["tpm"]
            .as_f64()
            .expect("animated TPM");
        let second_tpm = second["snapshots"][0]["telemetry"]["tpm"]
            .as_f64()
            .expect("animated TPM");
        assert!(second_tpm > first_tpm);
        assert!(second_tpm < 50_000.0);
    }

    #[test]
    fn encrypted_network_activity_drives_missing_provider_rates() {
        let mut value = json!({"snapshots": [{
            "provider": "groq",
            "confidence": "high",
            "telemetry": {"rpm": null, "tpm": 0.0, "rpd": null}
        }]});
        let activity = BTreeMap::from([(
            "groq".to_owned(),
            NetworkActivity {
                requests_per_minute: 3.0,
                estimated_tokens_per_minute: 1_200.0,
                active_connections: 1,
                detected_calls: 3,
                sent_bytes_per_second: 500.0,
                received_bytes_per_second: 800.0,
                source: "encrypted_network_estimate",
            },
        )]);
        apply_network_activity(&mut value, &activity);
        assert_eq!(value["snapshots"][0]["telemetry"]["rpm"], 3.0);
        assert_eq!(value["snapshots"][0]["telemetry"]["tpm"], 1_200.0);
        assert_eq!(value["snapshots"][0]["network_estimated"], true);
    }
}

fn main() {
    let raw_args: Vec<String> = std::env::args().collect();
    let mut positions: Vec<usize> = Vec::new();
    let mut fields: Vec<Vec<String>> = Vec::new();
    for (__i, __a) in raw_args.iter().enumerate() {
        if __a == "--config" {
            if let Some(__v) = raw_args.get(__i + 1) {
                positions.push(__i + 1);
                fields.push(curly_expand::expand_or_literal(__v));
            }
            break;
        } else if let Some(__v) = __a.strip_prefix("--config=") {
            positions.push(__i);
            fields.push(
                curly_expand::expand_or_literal(__v)
                    .into_iter()
                    .map(|v| format!("--config={}", v))
                    .collect(),
            );
            break;
        }
    }

    if fields.is_empty() || fields.iter().all(|f| f.len() <= 1) {
        __curly_original_main();
        return;
    }

    let combos = curly_expand::cartesian(&fields);
    let exe = std::env::current_exe().expect("resolve current exe");
    let mut had_failure = false;
    for combo in &combos {
        let mut new_args = raw_args.clone();
        for (slot, value) in positions.iter().zip(combo.iter()) {
            new_args[*slot] = value.clone();
        }
        let status = std::process::Command::new(&exe)
            .args(&new_args[1..])
            .status()
            .expect("failed to re-exec self");
        if !status.success() {
            had_failure = true;
        }
    }
    if had_failure {
        std::process::exit(1);
    }
}
