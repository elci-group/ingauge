// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use crate::error::AppError;
use crate::instrument::{
    odometer_digits, EngineState, GaugeConfig, TelemetryEvent, TelemetryFreshness, ZoneSeverity,
};
use chrono::Utc;
use serde_json::Value;
use std::{
    env,
    fmt::{self, Write as _},
    io::{self, IsTerminal, Write},
    thread,
    time::Duration,
};

const CYAN: &str = "38;2;88;213;255";
const GREEN: &str = "38;2;65;211;138";
const AMBER: &str = "38;2;255;176;32";
const RED: &str = "38;2;255;77;109";
const MUTED: &str = "38;2;130;148;166";
const WOOD: &str = "38;2;151;95;55";
const BOLD: &str = "1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProviderTheme {
    accent: &'static str,
    secondary: &'static str,
    badge: &'static str,
}

pub fn animate(command: &str) -> io::Result<()> {
    if !animation_enabled() {
        return Ok(());
    }
    let mut stderr = io::stderr().lock();
    for frame in ["◴", "◷", "◶", "◵"] {
        write!(
            stderr,
            "\r\x1b[2K\x1b[{CYAN}m{frame}\x1b[0m calibrating {}…",
            title(command)
        )?;
        stderr.flush()?;
        thread::sleep(Duration::from_millis(24));
    }
    write!(stderr, "\r\x1b[2K")?;
    stderr.flush()
}

pub fn render(command: &str, value: &Value) -> String {
    let styled = io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none();
    if command == "status" {
        return render_sports_car(value, styled);
    }
    let mut output = String::new();
    let icon = command_icon(command);
    let heading = format!("{icon} InGauge · {}", title(command));
    writeln!(output, "╭─ {}", paint(&heading, BOLD, styled)).record();
    writeln!(
        output,
        "│  {}",
        paint(concat!("v", env!("CARGO_PKG_VERSION")), MUTED, styled)
    )
    .record();
    render_value(value, 1, styled, &mut output);
    write!(
        output,
        "╰─ {}",
        paint("measurement complete ✓", GREEN, styled)
    )
    .record();
    output
}

fn render_sports_car(value: &Value, styled: bool) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "╭──────────────────────────────────────────────────────────────────────╮"
    )
    .record();
    let dashboard_title = format!("{:^70}", "INGAUGE  ◈  INFERENCE MOTORSPORT");
    writeln!(output, "│{}│", paint(&dashboard_title, BOLD, styled)).record();
    let grain = wood_grain(70);
    writeln!(output, "│{}│", paint(&grain, WOOD, styled)).record();
    writeln!(
        output,
        "├──────────────────────────────────────────────────────────────────────┤"
    )
    .record();
    if let Some(cycle) = value["cycle"].as_object() {
        let position = cycle.get("position").and_then(Value::as_u64).unwrap_or(1);
        let total = cycle.get("total").and_then(Value::as_u64).unwrap_or(1);
        let interval = cycle
            .get("interval_seconds")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let elapsed_ms = value["live"]["elapsed_ms"].as_u64().unwrap_or(0);
        let sample_seconds = value["live"]["sample_seconds"].as_u64().unwrap_or(0);
        let phase = value["live"]["phase"].as_str().unwrap_or("LIVE");
        let cycle_remaining = interval.saturating_sub((elapsed_ms / 1_000) % interval.max(1));
        let sample_remaining =
            sample_seconds.saturating_sub((elapsed_ms / 1_000) % sample_seconds.max(1));
        let pulse = if value["live"]["reduced_motion"].as_bool() == Some(true) {
            "◆"
        } else {
            ["◆", "◇", "◈", "◇"][(elapsed_ms / 250) as usize % 4]
        };
        let cycle_status = format!(
            "{pulse} {phase} · PROVIDER {position}/{total} · SWITCH {cycle_remaining}s · SAMPLE {sample_remaining}s · CTRL+C"
        );
        writeln!(output, "│ {:^68} │", cycle_status).record();
    }
    let telemetry_error = value["telemetry_error"].as_str();
    if let Some(error) = telemetry_error {
        let message = format!("TELEMETRY FAULT · {error}");
        let message = message.chars().take(68).collect::<String>();
        writeln!(
            output,
            "│ {} │",
            paint(&format!("{message:<68}"), RED, styled)
        )
        .record();
    }
    let snapshots = value["snapshots"].as_array().cloned().unwrap_or_default();
    let visible_snapshots = if snapshots.is_empty() {
        let configured = value["configured_providers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|provider| {
                serde_json::json!({
                    "provider": provider,
                    "model": if telemetry_error.is_some() { "telemetry unavailable" } else { "awaiting telemetry" },
                    "state": if telemetry_error.is_some() { "fault" } else { "offline" },
                    "freshness": "offline",
                    "telemetry": { "rpm": 0.0, "tpm": 0.0, "rpd": 0.0 }
                })
            })
            .collect::<Vec<_>>();
        if configured.is_empty() {
            vec![serde_json::json!({
                "provider": "NO TELEMETRY",
                "model": "configure a provider or start the daemon",
                "state": "offline",
                "freshness": "offline",
                "telemetry": { "rpm": 0.0, "tpm": 0.0, "rpd": 0.0 }
            })]
        } else {
            configured
        }
    } else {
        snapshots
    };
    for snapshot in &visible_snapshots {
        let provider = snapshot["provider"].as_str().unwrap_or("unknown");
        let theme = provider_theme(provider);
        let model = snapshot["model"].as_str().unwrap_or("provider-wide");
        let state = snapshot["state"].as_str().unwrap_or("unknown");
        let network_estimated = snapshot["network_estimated"].as_bool() == Some(true);
        let source = if network_estimated { " · NET EST" } else { "" };
        let title = format!("{}  {provider} / {model}  ·  {state}{source}", theme.badge);
        let title = format!("{title:<68}");
        writeln!(output, "│ {} │", paint(&title, theme.accent, styled)).record();
        let telemetry = &snapshot["telemetry"];
        let rpm = telemetry["rpm"].as_f64().unwrap_or(0.0);
        let tpm = telemetry["tpm"].as_f64().unwrap_or(0.0);
        let rpm_max = value["instruments"]["rpm"]["max"]
            .as_f64()
            .filter(|value| *value > 0.0)
            .unwrap_or(10_000.0);
        let rpm_redline = value["instruments"]["rpm"]["redline"]
            .as_f64()
            .unwrap_or(rpm_max * 0.85);
        let tpm_max = value["instruments"]["tpm"]["max"]
            .as_f64()
            .filter(|value| *value > 0.0)
            .unwrap_or(100_000.0);
        let tpm_redline = value["instruments"]["tpm"]["redline"]
            .as_f64()
            .unwrap_or(tpm_max * 0.85);
        let rpd = telemetry["rpd"].as_f64().unwrap_or(0.0);
        let rpd_limit = telemetry["rpd_limit"]
            .as_f64()
            .filter(|value| *value > 0.0)
            .or_else(|| value["instruments"]["rpd"]["daily_limit"].as_f64())
            .filter(|value| *value > 0.0)
            .unwrap_or(100_000.0);
        let event = TelemetryEvent {
            timestamp: Utc::now(),
            rpm: Some(rpm),
            tpm: Some(tpm),
            rpd: Some(rpd),
            lifetime_input: telemetry["lifetime_input_tokens"].as_u64(),
            lifetime_output: telemetry["lifetime_output_tokens"]
                .as_u64()
                .or_else(|| telemetry["output_tokens"].as_u64()),
            latency: None,
            provider: provider.to_owned(),
            status: match snapshot["freshness"].as_str() {
                Some("offline") => TelemetryFreshness::Offline,
                Some("stale") => TelemetryFreshness::Stale,
                _ => TelemetryFreshness::Live,
            },
        };
        let revs = dial_face(
            &format!("{} REVS", theme.badge),
            rpm,
            "RPM",
            &GaugeConfig::tachometer(rpm_max, rpm_redline),
            styled,
        );
        let speed = dial_face(
            &format!("{} SPEED", theme.badge),
            tpm,
            if network_estimated { "TPM~" } else { "TPM" },
            &GaugeConfig::speedometer(tpm_max, tpm_redline),
            styled,
        );
        let oil = oil_instrument(rpd, rpd_limit, styled);
        let mileage = odometer_instrument(
            event
                .lifetime_tokens()
                .or_else(|| telemetry["tokens_used"].as_u64()),
            styled,
        );
        for line in themed_pair_instruments(&revs, &speed, 3, theme.accent, theme.secondary, styled)
        {
            writeln!(output, "│ {line} │").record();
        }
        let calibration = format!(
            "CAL  RPM 0–{rpm_max:.0}  RED {rpm_redline:.0}  ◈  TPM 0–{tpm_max:.0}  RED {tpm_redline:.0}"
        );
        writeln!(output, "│ {:^68} │", calibration).record();
        let input = event
            .lifetime_input
            .map_or_else(|| "--".into(), |value| value.to_string());
        let output_tokens = event
            .lifetime_output
            .map_or_else(|| "--".into(), |value| value.to_string());
        let consumption = (rpd / rpd_limit * 100.0).clamp(0.0, 999.9);
        let detail = format!(
            "INPUT {input}  ◈  OUTPUT {output_tokens}  ◈  TODAY {rpd:.0} ({consumption:.1}%)"
        );
        writeln!(output, "│ {:^68} │", detail).record();
        if network_estimated {
            let calls = snapshot["network"]["requests_per_minute"]
                .as_f64()
                .unwrap_or(0.0);
            let received = snapshot["network"]["received_bytes_per_second"]
                .as_f64()
                .unwrap_or(0.0);
            let connections = snapshot["network"]["active_connections"]
                .as_u64()
                .unwrap_or(0);
            let network = format!(
                "NETWORK ESTIMATE  ◈  {calls:.0} CALLS/MIN  ◈  ↓ {received:.0} B/s  ◈  {connections} TLS"
            );
            writeln!(
                output,
                "│ {:^68} │",
                paint(&network, theme.secondary, styled)
            )
            .record();
        }
        writeln!(
            output,
            "│                                                                      │"
        )
        .record();
        for line in
            themed_pair_instruments(&oil, &mileage, 4, theme.secondary, theme.accent, styled)
        {
            writeln!(output, "│ {line} │").record();
        }
        let engine = event.engine_state(rpm_max, tpm_max, rpd_limit * 0.75);
        writeln!(
            output,
            "│ {:^68} │",
            warning_strip(engine, rpd / rpd_limit, styled)
        )
        .record();
        writeln!(output, "│{}│", paint(&grain, WOOD, styled)).record();
    }
    write!(
        output,
        "╰──────────────────────────────────────────────────────────────────────╯"
    )
    .record();
    output
}

fn wood_grain(width: usize) -> String {
    ["≈", "╱", "≈", "╲", "━", "≈", "╱", "·"]
        .into_iter()
        .cycle()
        .take(width)
        .collect()
}

const DIAL_WIDTH: usize = 32;
const DIAL_HEIGHT: usize = 15;

fn dial_face(
    label: &str,
    value: f64,
    unit: &str,
    config: &GaugeConfig,
    styled: bool,
) -> Vec<String> {
    let severity = config.severity_at(value);
    let color = match severity {
        ZoneSeverity::Normal => GREEN,
        ZoneSeverity::Performance => AMBER,
        ZoneSeverity::Warning | ZoneSeverity::Redline | ZoneSeverity::Critical => RED,
    };
    let mut face = vec![vec![' '; DIAL_WIDTH]; DIAL_HEIGHT];
    let center_x = 15_i32;
    let center_y = 7_i32;
    for degrees in 0..360 {
        let radians = f64::from(degrees).to_radians();
        put(
            &mut face,
            center_x + (14.0 * radians.sin()).round() as i32,
            center_y - (6.0 * radians.cos()).round() as i32,
            '·',
        );
    }
    for tick in 0..=config.major_ticks {
        let ratio = f64::from(tick) / f64::from(config.major_ticks.max(1));
        let angle = config.start_angle + ratio * (config.end_angle - config.start_angle);
        let radians = angle.to_radians();
        put(
            &mut face,
            center_x + (12.0 * radians.sin()).round() as i32,
            center_y - (5.0 * radians.cos()).round() as i32,
            '┃',
        );
    }
    let redline_angle = config
        .zones
        .iter()
        .find(|zone| zone.severity == ZoneSeverity::Redline)
        .map_or(config.end_angle, |zone| config.value_to_angle(zone.start));
    for degree in (redline_angle.round() as i32)..=(config.end_angle.round() as i32) {
        let radians = f64::from(degree).to_radians();
        put(
            &mut face,
            center_x + (10.8 * radians.sin()).round() as i32,
            center_y - (4.5 * radians.cos()).round() as i32,
            '▪',
        );
    }
    let needle_angle = config.value_to_angle(value).to_radians();
    let end_x = center_x + (9.0 * needle_angle.sin()).round() as i32;
    let end_y = center_y - (4.0 * needle_angle.cos()).round() as i32;
    draw_needle(&mut face, center_x, center_y, end_x, end_y);
    put(&mut face, center_x, center_y, '●');
    put_text(&mut face, 0, 1, label);
    put_centered(&mut face, 10, &format!("{value:.1}"));
    put_centered(&mut face, 11, unit);
    put_text(&mut face, 12, 2, "0");
    let maximum = format!("{:.0}", config.max);
    put_text(
        &mut face,
        12,
        DIAL_WIDTH.saturating_sub(maximum.len() + 2),
        &maximum,
    );
    let _dial_color = (color, styled);
    face.into_iter()
        .map(|row| row.iter().collect::<String>().trim_end().to_owned())
        .collect()
}

fn put(face: &mut [Vec<char>], x: i32, y: i32, value: char) {
    if x >= 0 && y >= 0 {
        if let Some(cell) = face
            .get_mut(y as usize)
            .and_then(|row| row.get_mut(x as usize))
        {
            *cell = value;
        }
    }
}

fn put_text(face: &mut [Vec<char>], y: usize, x: usize, text: &str) {
    for (offset, character) in text.chars().enumerate() {
        if let Some(cell) = face.get_mut(y).and_then(|row| row.get_mut(x + offset)) {
            *cell = character;
        }
    }
}

fn put_centered(face: &mut [Vec<char>], y: usize, text: &str) {
    put_text(
        face,
        y,
        DIAL_WIDTH.saturating_sub(text.chars().count()) / 2,
        text,
    );
}

fn draw_needle(face: &mut [Vec<char>], mut x: i32, mut y: i32, end_x: i32, end_y: i32) {
    let dx = (end_x - x).abs();
    let sx = if x < end_x { 1 } else { -1 };
    let dy = -(end_y - y).abs();
    let sy = if y < end_y { 1 } else { -1 };
    let mut error = dx + dy;
    let glyph = if dx > -dy * 2 {
        '━'
    } else if -dy > dx * 2 {
        '┃'
    } else if sx == sy {
        '╲'
    } else {
        '╱'
    };
    loop {
        put(face, x, y, glyph);
        if x == end_x && y == end_y {
            break;
        }
        let doubled = 2 * error;
        if doubled >= dy {
            error += dy;
            x += sx;
        }
        if doubled <= dx {
            error += dx;
            y += sy;
        }
    }
}

fn oil_instrument(value: f64, limit: f64, _styled: bool) -> Vec<String> {
    let config = GaugeConfig::oil(limit, limit * 0.75, limit * 0.9);
    let severity = config.severity_at(value);
    let state = match severity {
        ZoneSeverity::Normal => "NORMAL",
        ZoneSeverity::Warning => "WARNING",
        ZoneSeverity::Critical => "CRITICAL",
        _ => "NORMAL",
    };
    let ratio = (value / limit).clamp(0.0, 1.0);
    let needle = (ratio * 17.0).round() as usize;
    let scale = (0..18)
        .map(|index| if index == needle { '▲' } else { '─' })
        .collect::<String>();
    vec![
        "╭───────── OIL / RPD ──────────╮".into(),
        format!("│ LOW  {scale} HIGH │"),
        format!("│ {:>7.0} / {:<7.0} RPD        │", value, limit),
        format!("│{state:^30}│"),
        "╰──── provider reset window ───╯".into(),
    ]
}

fn odometer_instrument(value: Option<u64>, _styled: bool) -> Vec<String> {
    match value {
        Some(value) => {
            let wheels = odometer_digits(value, 12)
                .into_iter()
                .map(|digit| digit.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            vec![
                "╭────── LIFETIME MILEAGE ──────╮".into(),
                "│                              │".into(),
                format!("│{wheels:^30}│"),
                "│            TOKENS            │".into(),
                "╰──────────────────────────────╯".into(),
            ]
        }
        None => vec![
            "╭────── LIFETIME MILEAGE ──────╮".into(),
            "│                              │".into(),
            "│  — — — — — — — — — — — —  │".into(),
            "│             STALE            │".into(),
            "╰──────────────────────────────╯".into(),
        ],
    }
}

fn themed_pair_instruments(
    left: &[String],
    right: &[String],
    gap: usize,
    left_color: &str,
    right_color: &str,
    styled: bool,
) -> Vec<String> {
    let lines = left.len().max(right.len());
    (0..lines)
        .map(|line| {
            let left = left.get(line).map(String::as_str).unwrap_or("");
            let right = right.get(line).map(String::as_str).unwrap_or("");
            let visible_width = DIAL_WIDTH * 2 + gap;
            format!(
                "{}{}{}{}",
                paint(&format!("{left:<DIAL_WIDTH$}"), left_color, styled),
                " ".repeat(gap),
                paint(&format!("{right:<DIAL_WIDTH$}"), right_color, styled),
                " ".repeat(68_usize.saturating_sub(visible_width))
            )
        })
        .collect()
}

fn provider_theme(provider: &str) -> ProviderTheme {
    let normalized = provider.to_ascii_lowercase();
    if normalized.contains("openai") {
        return ProviderTheme {
            accent: "38;2;16;163;127",
            secondary: "38;2;141;235;208",
            badge: "OPENAI",
        };
    }
    if normalized.contains("anthropic") || normalized.contains("claude") {
        return ProviderTheme {
            accent: "38;2;214;123;76",
            secondary: "38;2;244;201;168",
            badge: "CLAUDE",
        };
    }
    if normalized.contains("groq") {
        return ProviderTheme {
            accent: "38;2;244;63;94",
            secondary: "38;2;255;176;32",
            badge: "GROQ",
        };
    }
    if normalized.contains("gemini") || normalized.contains("google") {
        return ProviderTheme {
            accent: "38;2;66;133;244",
            secondary: "38;2;72;207;173",
            badge: "GEMINI",
        };
    }
    if normalized.contains("openrouter") {
        return ProviderTheme {
            accent: "38;2;154;111;255",
            secondary: "38;2;236;111;255",
            badge: "ROUTER",
        };
    }
    const FALLBACKS: [ProviderTheme; 4] = [
        ProviderTheme {
            accent: CYAN,
            secondary: GREEN,
            badge: "CUSTOM",
        },
        ProviderTheme {
            accent: AMBER,
            secondary: "38;2;255;218;121",
            badge: "CUSTOM",
        },
        ProviderTheme {
            accent: "38;2;194;120;255",
            secondary: "38;2;255;121;198",
            badge: "CUSTOM",
        },
        ProviderTheme {
            accent: "38;2;87;209;182",
            secondary: "38;2;99;166;255",
            badge: "CUSTOM",
        },
    ];
    let hash = normalized.bytes().fold(0_usize, |hash, byte| {
        hash.wrapping_mul(31) + usize::from(byte)
    });
    FALLBACKS[hash % FALLBACKS.len()]
}

fn warning_strip(engine: EngineState, rpd_ratio: f64, _styled: bool) -> String {
    let engine_lamp = if engine == EngineState::Fault {
        "●"
    } else {
        "○"
    };
    let oil_lamp = if rpd_ratio >= 0.75 { "●" } else { "○" };
    let network_lamp = if engine == EngineState::Fault {
        "●"
    } else {
        "○"
    };
    format!(
        "{engine_lamp} ENGINE  {oil_lamp} OIL  ○ TEMP  ○ BRAKE  ○ SERVICE  {network_lamp} NETWORK"
    )
}

pub fn render_error(error: &AppError) -> String {
    let styled = io::stderr().is_terminal() && env::var_os("NO_COLOR").is_none();
    format!(
        "╭─ {}\n│  {}\n│  {}\n╰─ {}",
        paint("⛔ InGauge · Action needed", RED, styled),
        paint(error.code(), MUTED, styled),
        error,
        paint(error_hint(error), AMBER, styled)
    )
}

fn render_value(value: &Value, depth: usize, styled: bool, output: &mut String) {
    let prefix = format!("│  {}", "  ".repeat(depth.saturating_sub(1)));
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let label = format!("{} {}", key_icon(key), humanize(key));
                if is_scalar(child) {
                    writeln!(
                        output,
                        "{prefix}{}  {}",
                        paint(&label, CYAN, styled),
                        scalar(child, styled)
                    )
                    .record();
                } else {
                    writeln!(output, "{prefix}{}", paint(&label, CYAN, styled)).record();
                    render_value(child, depth + 1, styled, output);
                }
            }
        }
        Value::Array(items) if items.is_empty() => {
            writeln!(output, "{prefix}{}", paint("◇ none", MUTED, styled)).record();
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                if is_scalar(child) {
                    writeln!(output, "{prefix}• {}", scalar(child, styled)).record();
                } else {
                    writeln!(
                        output,
                        "{prefix}{}",
                        paint(&format!("◆ {}", index + 1), AMBER, styled)
                    )
                    .record();
                    render_value(child, depth + 1, styled, output);
                }
            }
        }
        scalar_value => {
            writeln!(output, "{prefix}{}", scalar(scalar_value, styled)).record();
        }
    }
}

trait RecordFormat {
    fn record(self);
}

impl RecordFormat for fmt::Result {
    fn record(self) {
        if let Err(error) = self {
            tracing::error!(
                operation = "render_terminal_presentation",
                %error,
                "failed to render terminal presentation"
            );
        }
    }
}

fn scalar(value: &Value, styled: bool) -> String {
    match value {
        Value::String(text) => {
            let (icon, color) = match text.as_str() {
                "healthy" | "authoritative" | "high" => ("✅ ", GREEN),
                "moderate" | "medium" | "stale" => ("⚠️  ", AMBER),
                "critical" | "exhausted" => ("🚨 ", RED),
                "unknown" | "low" => ("❔ ", MUTED),
                _ => ("", ""),
            };
            paint(&format!("{icon}{text}"), color, styled)
        }
        Value::Bool(true) => paint("✅ yes", GREEN, styled),
        Value::Bool(false) => paint("○ no", MUTED, styled),
        Value::Null => paint("◇ unavailable", MUTED, styled),
        other => other.to_string(),
    }
}

fn is_scalar(value: &Value) -> bool {
    !matches!(value, Value::Array(_) | Value::Object(_))
}

fn paint(text: &str, code: &str, enabled: bool) -> String {
    if enabled && !code.is_empty() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_owned()
    }
}

fn animation_enabled() -> bool {
    animation_allowed(
        io::stderr().is_terminal(),
        env::var_os("NO_COLOR").is_some(),
        env::var_os("CI").is_some(),
        env::var_os("INGAUGE_NO_ANIMATION").is_some(),
        env::var("TERM").is_ok_and(|term| term == "dumb"),
    )
}

fn animation_allowed(
    terminal: bool,
    no_color: bool,
    continuous_integration: bool,
    reduced_motion: bool,
    dumb_terminal: bool,
) -> bool {
    terminal && !no_color && !continuous_integration && !reduced_motion && !dumb_terminal
}

fn title(command: &str) -> String {
    humanize(command)
}

fn humanize(value: &str) -> String {
    let mut words = value.replace('_', " ");
    if let Some(first) = words.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    words
}

fn command_icon(command: &str) -> &'static str {
    match command {
        "status" => "📊",
        "providers" | "discover" => "🛰️",
        "probe" => "📡",
        "history" => "🗂️",
        "forecast" | "next" => "🔭",
        "health" => "💚",
        "config_validate" => "✅",
        "db" => "🗄️",
        "export_padagonia" => "🕸️",
        _ => "⚙️",
    }
}

fn key_icon(key: &str) -> &'static str {
    match key {
        "status" | "state" => "◉",
        "provider" | "providers" => "☁️",
        "model" => "🧠",
        "snapshots" => "📸",
        "events" => "⚡",
        "observations" => "📈",
        "forecast" => "🔭",
        "heartbeat_at" | "generated_at" | "observed_at" => "🕒",
        "remaining" | "headroom" => "⛽",
        "errors" => "⛔",
        "warnings" => "⚠️",
        _ => "›",
    }
}

fn error_hint(error: &AppError) -> &'static str {
    match error {
        AppError::Configuration(_) => "💡 Check `ingauge config validate`, then try again.",
        AppError::Provider(_) => "💡 Check the target endpoint and credential environment.",
        AppError::Storage(_) => "💡 Run `ingauge db integrity` and inspect database permissions.",
        AppError::Io(_) => "💡 Check the referenced path and filesystem permissions.",
        AppError::Serialization(_) => "💡 Retry with `--json`; report the input if this persists.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn human_output_is_structured_emojified_and_plain_when_piped() {
        let output = render("health", &json!({"status":"healthy","snapshots":[]}));
        assert!(output.contains("💚 InGauge · Health"));
        assert!(output.contains("✅ healthy"));
        assert!(output.contains("📸 Snapshots"));
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn empty_status_still_renders_an_offline_cockpit() {
        let output = render("status", &json!({"snapshots":[], "events":[]}));
        assert!(output.contains("INFERENCE MOTORSPORT"));
        assert!(output.contains("NO TELEMETRY"));
        assert!(output.contains("offline"));
        assert!(output
            .lines()
            .any(|line| line.contains("REVS") && line.contains("SPEED")));
        assert!(output.contains("● ENGINE"));
        assert!(output.contains("● NETWORK"));
        assert!(!output.contains("› Instruments"));
    }

    #[test]
    fn status_uses_the_four_instrument_cockpit_for_model_telemetry() {
        let output = render(
            "status",
            &json!({"snapshots":[{"provider":"groq","model":"m1","state":"healthy","telemetry":{
                "fuel_used":25,"fuel_limit":100,"tokens_used":2500,"output_tokens":40,"tpm_limit":100,
                "responses":2,"rpm_limit":10,"rpd":20,"rpd_limit":100}}]}),
        );
        assert!(output.contains("INFERENCE MOTORSPORT"));
        assert!(output.contains("REVS"));
        assert!(output.contains("SPEED"));
        assert!(output.contains("OIL / RPD"));
        assert!(output.contains("LIFETIME MILEAGE"));
        assert!(output.contains("CAL  RPM"));
        assert!(output.contains("TODAY"));
        assert!(output.contains("≈╱≈╲━"));
        assert!(output.contains("ENGINE"));
        assert!(output.contains('●'));
        assert!(output.contains('┃'));
        assert!(
            output
                .lines()
                .any(|line| line.contains("REVS") && line.contains("SPEED")),
            "{output}"
        );
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn provider_identity_changes_both_palette_and_dial_labels() {
        let openai = provider_theme("openai");
        let anthropic = provider_theme("anthropic");
        let groq = provider_theme("groq");
        let gemini = provider_theme("gemini");
        let router = provider_theme("openrouter");
        assert_ne!(openai.accent, anthropic.accent);
        assert_ne!(anthropic.accent, groq.accent);
        assert_ne!(groq.accent, gemini.accent);
        assert_ne!(gemini.accent, router.accent);

        let output = render_sports_car(
            &json!({
                "snapshots": [{
                    "provider": "anthropic",
                    "model": "claude",
                    "state": "healthy",
                    "telemetry": {"rpm": 1.0, "tpm": 2.0, "rpd": 3.0}
                }],
                "cycle": {"position": 2, "total": 5, "interval_seconds": 4}
            }),
            true,
        );
        assert!(output.contains("CLAUDE REVS"));
        assert!(output.contains("CLAUDE SPEED"));
        assert!(output.contains("PROVIDER 2/5"));
        assert!(output.contains(&format!("\x1b[{}m", anthropic.accent)));
    }

    #[test]
    fn custom_provider_palette_is_stable() {
        assert_eq!(
            provider_theme("local-router"),
            provider_theme("local-router")
        );
    }

    #[test]
    fn network_estimates_are_visibly_distinguished_from_token_telemetry() {
        let output = render(
            "status",
            &json!({"snapshots": [{
                "provider": "groq",
                "model": "encrypted network traffic",
                "state": "accelerating",
                "network_estimated": true,
                "network": {
                    "requests_per_minute": 2.0,
                    "received_bytes_per_second": 900.0,
                    "active_connections": 1
                },
                "telemetry": {"rpm": 2.0, "tpm": 1200.0, "rpd": 0.0}
            }]}),
        );
        assert!(output.contains("NET EST"));
        assert!(output.contains("TPM~"));
        assert!(output.contains("NETWORK ESTIMATE"));
        assert!(output.contains("2 CALLS/MIN"));
    }

    #[test]
    fn dial_face_encodes_value_in_needle_geometry() {
        let config = GaugeConfig::tachometer(10_000.0, 8_500.0);
        let idle = dial_face("REVS", 0.0, "RPM", &config, false);
        let redline = dial_face("REVS", 9_000.0, "RPM", &config, false);
        assert_ne!(idle, redline);
        assert!(redline.iter().any(|line| line.contains('▪')));
        assert!(redline.iter().any(|line| line.contains('●')));
        assert!(redline
            .iter()
            .all(|line| line.chars().count() <= DIAL_WIDTH));
    }

    #[test]
    fn auxiliary_instruments_have_matching_dashboard_widths() {
        let oil = oil_instrument(20.0, 100.0, false);
        let mileage = odometer_instrument(Some(2_500), false);
        let oil_widths = oil
            .iter()
            .map(|line| line.chars().count())
            .collect::<Vec<_>>();
        assert!(
            oil_widths.iter().all(|width| *width == DIAL_WIDTH),
            "{oil_widths:?}"
        );
        let mileage_widths = mileage
            .iter()
            .map(|line| line.chars().count())
            .collect::<Vec<_>>();
        assert!(
            mileage_widths.iter().all(|width| *width == DIAL_WIDTH),
            "{mileage_widths:?}"
        );
    }

    #[test]
    fn wood_grain_is_deterministic_and_width_bounded() {
        let grain = wood_grain(70);
        assert_eq!(grain.chars().count(), 70);
        assert_eq!(grain, wood_grain(70));
    }

    #[test]
    fn errors_include_a_specific_recovery_action() {
        let output = render_error(&AppError::Configuration("invalid input".into()));
        assert!(output.contains("⛔ InGauge"));
        assert!(output.contains("config validate"));
    }

    #[test]
    fn animation_requires_an_interactive_opted_in_terminal() {
        assert!(animation_allowed(true, false, false, false, false));
        assert!(!animation_allowed(false, false, false, false, false));
        assert!(!animation_allowed(true, true, false, false, false));
        assert!(!animation_allowed(true, false, true, false, false));
        assert!(!animation_allowed(true, false, false, true, false));
        assert!(!animation_allowed(true, false, false, false, true));
    }
}
