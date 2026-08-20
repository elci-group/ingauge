#![allow(clippy::unwrap_used)]

use chrono::{Duration, Utc};
use ingauge::{store::Store, Confidence, Metric, MetricValue, Observation, ObservationSource};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::{Child, Command},
    thread,
    time::Duration as StdDuration,
};
use tempfile::TempDir;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ingauge"))
}

fn fixture() -> (TempDir, std::path::PathBuf) {
    fixture_with("")
}

fn fixture_with(extra: &str) -> (TempDir, std::path::PathBuf) {
    let directory = TempDir::new().expect("temp directory");
    let config = directory.path().join("ingauge.toml");
    let database = directory.path().join("history.db");
    fs::write(
        &config,
        format!(
            "schema_version = 1\n[general]\ndatabase = {:?}\npoll_interval = \"1s\"\nhistory_retention = \"1d\"\n{extra}",
            database.to_string_lossy(),
        ),
    )
    .expect("write config");
    (directory, config)
}

fn run(config: &std::path::Path, args: &[&str]) -> std::process::Output {
    let mut command = binary();
    command.args(["--json", "--config", config.to_str().unwrap()]);
    command.args(args);
    command.output().expect("run binary")
}

fn seed(config: &std::path::Path) {
    let text = fs::read_to_string(config).expect("read config");
    let config: ingauge::config::Config = toml::from_str(&text).expect("parse config");
    let store = Store::open(config.general.database.expect("database path")).expect("open store");
    let now = Utc::now();
    for index in 0..6_u64 {
        store
            .insert(&Observation {
                provider: "fixture".into(),
                model: Some("m1".into()),
                metric: Metric::Tokens,
                value: MetricValue::Integer(index * 100),
                observed_at: now + Duration::minutes(index as i64),
                source: ObservationSource::Fixture,
                confidence: Confidence::High,
            })
            .expect("seed observation");
    }
}

#[test]
fn config_validate_and_json_status_have_stable_contracts() {
    let (_directory, config) = fixture();
    let output = binary()
        .args(["--config", config.to_str().unwrap(), "config", "validate"])
        .output()
        .expect("run binary");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "configuration valid"
    );

    let output = binary()
        .args(["--json", "--config", config.to_str().unwrap(), "status"])
        .output()
        .expect("run binary");
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(body["schema_version"], 1);
    assert_eq!(body["command"], "status");
    assert!(body["errors"].as_array().is_some_and(Vec::is_empty));
}

#[test]
fn database_commands_initialize_and_check_storage() {
    let (_directory, config) = fixture();
    let migrate = binary()
        .args(["--config", config.to_str().unwrap(), "db", "migrate"])
        .output()
        .expect("run migration");
    assert!(migrate.status.success());
    let integrity = binary()
        .args([
            "--json",
            "--config",
            config.to_str().unwrap(),
            "db",
            "integrity",
        ])
        .output()
        .expect("run integrity");
    assert!(integrity.status.success());
    let body: Value = serde_json::from_slice(&integrity.stdout).expect("valid JSON");
    assert_eq!(body["data"]["result"], "ok");
}

#[test]
fn invalid_configuration_uses_stable_exit_code_and_json_error() {
    let directory = TempDir::new().expect("temp directory");
    let config = directory.path().join("bad.toml");
    fs::write(&config, "schema_version = 99\n").expect("write config");
    let output = binary()
        .args(["--config", config.to_str().unwrap(), "status"])
        .output()
        .expect("run binary");
    assert_eq!(output.status.code(), Some(3));
    let body: Value = serde_json::from_slice(&output.stderr).expect("valid JSON error");
    assert_eq!(body["errors"][0]["code"], "configuration_error");
}

#[test]
fn history_forecast_next_health_and_export_commands_execute() {
    let (directory, config) = fixture();
    seed(&config);
    for args in [
        vec!["history", "--provider", "fixture", "--limit", "3"],
        vec!["forecast", "--provider", "fixture", "--model", "m1"],
        vec!["next"],
        vec!["providers"],
        vec!["health"],
        vec!["db", "checkpoint"],
    ] {
        let output = run(&config, &args);
        assert!(
            output.status.success(),
            "command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let _: Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    }
    let backup = directory.path().join("backup.db");
    let output = run(&config, &["db", "backup", backup.to_str().unwrap()]);
    assert!(output.status.success());
    assert!(backup.exists());
    let graph = directory.path().join("graph.json");
    let output = run(
        &config,
        &[
            "export",
            "padagonia",
            "--output",
            graph.to_str().unwrap(),
            "--since",
            "1d",
        ],
    );
    assert!(output.status.success());
    let projection: Value =
        serde_json::from_slice(&fs::read(graph).expect("read graph")).expect("valid graph");
    assert!(projection["nodes"]
        .as_array()
        .is_some_and(|nodes| !nodes.is_empty()));
}

#[test]
fn live_probe_parses_bounded_harness_response() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
    let address = listener.local_addr().expect("fixture address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).expect("read request");
        let body = r#"[{"provider":"fixture","model":"m1","used":5,"limit":10,"remaining":5,"tokens_per_minute":0.5}]"#;
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).expect("write response");
    });
    let extra = format!("max_attempts = 1\n[providers.harness]\nendpoint = \"http://{address}\"\n");
    let (_directory, config) = fixture_with(&extra);
    let output = run(&config, &["probe"]);
    server.join().expect("server thread");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body: Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(body["data"]["observations"][3]["value"], 0.5);
}

#[test]
fn multiple_provider_and_router_bridges_are_probed_together() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
    let address = listener.local_addr().expect("fixture address");
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0_u8; 2048];
            let count = stream.read(&mut request).expect("read request");
            assert!(
                String::from_utf8_lossy(&request[..count]).starts_with("GET /capacity HTTP/1.1")
            );
            let body = r#"[{"provider":"upstream","used":1,"limit":10}]"#;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).expect("write response");
        }
    });
    let extra = format!(
        "max_attempts = 1\n[providers.groq]\nendpoint = \"http://{address}\"\nusage_path = \"/capacity\"\n[providers.vico-router]\nendpoint = \"http://{address}\"\nusage_path = \"/capacity\"\n"
    );
    let (_directory, config) = fixture_with(&extra);
    let output = run(&config, &["probe"]);
    server.join().expect("server thread");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body: Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(
        body["data"]["observations"].as_array().map(Vec::len),
        Some(4)
    );
}

#[test]
fn discover_inventories_dynamic_harness_manifests() {
    let (directory, config) = fixture();
    let harnesses = directory.path().join("harnesses");
    fs::create_dir(&harnesses).expect("create harness directory");
    fs::write(
        harnesses.join("Future.json"),
        br#"{"agent_name":"FutureHarness","installed":true,"configured":true,"settings":{"token":"must-not-leak"}}"#,
    )
    .expect("write harness manifest");
    let output = run(
        &config,
        &[
            "discover",
            "--harness-directory",
            harnesses.to_str().unwrap(),
        ],
    );
    assert!(output.status.success());
    let body: Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let rendered = serde_json::to_string(&body).expect("render inventory");
    assert!(rendered.contains("FutureHarness"));
    assert!(!rendered.contains("must-not-leak"));
}

#[test]
fn daemon_writes_heartbeat_and_stops_on_sigterm() {
    let (_directory, config) = fixture();
    let mut child: Child = binary()
        .args(["--config", config.to_str().unwrap(), "daemon"])
        .spawn()
        .expect("start daemon");
    thread::sleep(StdDuration::from_millis(1200));
    let status = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("signal daemon");
    assert!(status.success());
    assert!(child.wait().expect("wait daemon").success());
    let output = run(&config, &["health"]);
    let body: Value = serde_json::from_slice(&output.stdout).expect("valid health JSON");
    assert_eq!(body["data"]["status"], "healthy");
}
