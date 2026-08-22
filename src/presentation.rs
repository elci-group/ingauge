// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use crate::error::AppError;
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
const BOLD: &str = "1";

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
        let output = render("status", &json!({"status":"healthy","snapshots":[]}));
        assert!(output.contains("📊 InGauge · Status"));
        assert!(output.contains("✅ healthy"));
        assert!(output.contains("📸 Snapshots"));
        assert!(!output.contains("\x1b["));
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
