use chrono::Utc;
use clap::{Parser, Subcommand};
use ingauge::{
    capacity,
    config::Config,
    forecast,
    providers::{HarnessAdapter, ProbeContext, ProviderAdapter},
};
#[derive(Parser)]
#[command(
    name = "ingauge",
    version,
    about = "Inference-capacity observability and forecasting"
)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    watch: bool,
    #[arg(long, global = true)]
    config: Option<std::path::PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    Status,
    Providers,
    Probe,
    History,
    Forecast,
    Next,
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}
#[derive(Subcommand)]
enum ConfigCommand {
    Validate,
}
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_ansi(false)
        .try_init()
        .ok();
    let cli = Cli::parse();
    let cfg = match Config::load(cli.config.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(3)
        }
    };
    if let Err(e) = cfg.validate() {
        eprintln!("{e}");
        std::process::exit(3)
    }
    if matches!(
        cli.command,
        Some(Command::Config {
            command: ConfigCommand::Validate
        })
    ) {
        println!("configuration valid");
        return;
    }
    let mut snaps = Vec::new();
    if let Some(p) = cfg.providers.get("harness") {
        if p.enabled.unwrap_or(true) {
            let adapter = HarnessAdapter {
                endpoint: p
                    .endpoint
                    .clone()
                    .unwrap_or_else(|| "http://127.0.0.1:3000".into()),
            };
            let ctx = ProbeContext {
                client: reqwest::Client::new(),
                now: Utc::now(),
            };
            match adapter.probe(&ctx).await {
                Ok(s) => {
                    snaps = capacity::snapshots(&s.observations);
                    if let Some(path) = cfg.general.database.as_deref() {
                        if let Ok(store) = ingauge::store::Store::open(path) {
                            for o in s.observations {
                                let _ = store.insert(&o);
                            }
                        }
                    }
                }
                Err(e) => {
                    if cli.json {
                        println!("{{\"error\":{}}}", serde_json::to_string(&e).unwrap())
                    } else {
                        println!("InGauge\n\nHARNESS  ✗ {e}");
                    }
                    if !cli.json {
                        std::process::exit(4)
                    }
                    return;
                }
            }
        }
    }
    if cli.json {
        println!("{}",serde_json::to_string_pretty(&serde_json::json!({"version":"0.1.0","observed_at":Utc::now(),"snapshots":snaps,"events":forecast::events(&snaps)})).unwrap());
    } else {
        println!(
            "InGauge v0.1.0\nInference capacity · {}\n",
            Utc::now().format("%H:%M UTC")
        );
        if snaps.is_empty() {
            println!("No enabled providers returned capacity data. Configure [providers.harness] or use --json.");
        }
        for s in &snaps {
            println!(
                "{:<12} {:<18} {:>5.0}%  {:?}  {}",
                s.provider,
                s.model.as_ref().map(|m| m.0.as_str()).unwrap_or("—"),
                s.headroom * 100.,
                s.state,
                s.next_reset
                    .map(|x| x.format("%H:%M").to_string())
                    .unwrap_or_else(|| "—".into())
            );
        }
        let es = forecast::events(&snaps);
        if !es.is_empty() {
            println!("\nNEXT CAPACITY EVENTS");
            for e in es {
                println!("{}  {:<12} {:?}", e.at.format("%H:%M"), e.provider, e.kind)
            }
        }
    }
    if cli.watch {
        let _ = cli.watch;
    }
}
