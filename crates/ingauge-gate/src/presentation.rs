use std::{
    env,
    io::{self, IsTerminal},
    time::{Duration, Instant},
};

const CYAN: &str = "38;2;88;213;255";
const GREEN: &str = "38;2;65;211;138";
const AMBER: &str = "38;2;255;176;32";

pub(crate) async fn wait(target: &str, reason: &str, duration: Duration, show_timer: bool) {
    let styled = timer_style_enabled();
    let animated = show_timer && timer_animation_enabled();
    if show_timer {
        eprint!("{}", delay_line(target, reason, None, styled));
    }

    if animated {
        animate_delay(target, reason, duration, styled).await;
    } else {
        tokio::time::sleep(duration).await;
    }

    if show_timer {
        let clear = if animated { "\r\x1b[2K" } else { "\n" };
        eprintln!("{clear}{}", admitted_line(target, styled));
    }
}

async fn animate_delay(target: &str, reason: &str, duration: Duration, styled: bool) {
    let end = Instant::now() + duration;
    let mut frame = 0_usize;
    while Instant::now() < end {
        let remaining = end - Instant::now();
        eprint!(
            "\r\x1b[2K{}",
            animated_delay_line(target, reason, remaining.as_secs_f64(), frame, styled)
        );
        frame = frame.wrapping_add(1);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn timer_style_enabled() -> bool {
    io::stderr().is_terminal()
        && env::var_os("NO_COLOR").is_none()
        && !env::var("TERM").is_ok_and(|term| term == "dumb")
}

fn timer_animation_enabled() -> bool {
    timer_style_enabled()
        && env::var_os("CI").is_none()
        && env::var_os("INGAUGE_NO_ANIMATION").is_none()
}

fn delay_line(target: &str, reason: &str, remaining: Option<f64>, styled: bool) -> String {
    let countdown = remaining.map_or_else(String::new, |seconds| format!(" · {seconds:.1}s"));
    format!(
        "{} {} · delaying {target} · {reason}{countdown}",
        paint("⏳", AMBER, styled),
        paint("InGauge", CYAN, styled),
    )
}

fn animated_delay_line(
    target: &str,
    reason: &str,
    remaining: f64,
    frame: usize,
    styled: bool,
) -> String {
    let spinner = ["◴", "◷", "◶", "◵"][frame % 4];
    delay_line(target, reason, Some(remaining), styled).replacen(
        '⏳',
        &paint(spinner, AMBER, styled),
        1,
    )
}

fn admitted_line(target: &str, styled: bool) -> String {
    format!(
        "{} {} · admitted {target} · capacity ready",
        paint("✅", GREEN, styled),
        paint("InGauge", CYAN, styled),
    )
}

fn paint(text: &str, color: &str, styled: bool) -> String {
    if styled {
        format!("\x1b[{color}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_output_is_styled_emojified_and_plain_capable() {
        let plain = delay_line("groq/model", "capacity constrained", Some(1.25), false);
        assert_eq!(
            plain,
            "⏳ InGauge · delaying groq/model · capacity constrained · 1.2s"
        );
        assert!(!plain.contains("\x1b["));

        let styled = admitted_line("groq/model", true);
        assert!(styled.contains("✅"));
        assert!(styled.contains("capacity ready"));
        assert!(styled.contains("\x1b["));
    }
}
