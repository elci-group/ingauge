// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
//! Telemetry-independent automotive instrument primitives.
//!
//! The presentation layer supplies terminal, web, or native rendering. This
//! module only maps values into calibrated geometry and physical state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ZoneSeverity {
    Normal,
    Performance,
    Warning,
    Redline,
    Critical,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GaugeZone {
    pub start: f64,
    pub end: f64,
    pub severity: ZoneSeverity,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GaugeConfig {
    pub min: f64,
    pub max: f64,
    /// Clockwise angle, in degrees, at the minimum value.
    pub start_angle: f64,
    /// Clockwise angle, in degrees, at the maximum value.
    pub end_angle: f64,
    pub major_ticks: u16,
    pub minor_ticks_per_major: u16,
    pub zones: Vec<GaugeZone>,
}

impl GaugeConfig {
    pub fn tachometer(max: f64, redline: f64) -> Self {
        Self {
            min: 0.0,
            max: max.max(1.0),
            start_angle: -135.0,
            end_angle: 135.0,
            major_ticks: 10,
            minor_ticks_per_major: 4,
            zones: vec![
                GaugeZone {
                    start: 0.0,
                    end: redline * 0.75,
                    severity: ZoneSeverity::Normal,
                },
                GaugeZone {
                    start: redline * 0.75,
                    end: redline,
                    severity: ZoneSeverity::Performance,
                },
                GaugeZone {
                    start: redline,
                    end: max.max(redline),
                    severity: ZoneSeverity::Redline,
                },
            ],
        }
    }

    pub fn speedometer(max: f64, redline: f64) -> Self {
        Self {
            zones: vec![
                GaugeZone {
                    start: 0.0,
                    end: redline * 0.75,
                    severity: ZoneSeverity::Normal,
                },
                GaugeZone {
                    start: redline * 0.75,
                    end: redline,
                    severity: ZoneSeverity::Performance,
                },
                GaugeZone {
                    start: redline,
                    end: max.max(redline),
                    severity: ZoneSeverity::Redline,
                },
            ],
            ..Self::tachometer(max, redline)
        }
    }

    pub fn oil(limit: f64, warning: f64, critical: f64) -> Self {
        Self {
            min: 0.0,
            max: limit.max(1.0),
            start_angle: -120.0,
            end_angle: 120.0,
            major_ticks: 4,
            minor_ticks_per_major: 2,
            zones: vec![
                GaugeZone {
                    start: 0.0,
                    end: warning,
                    severity: ZoneSeverity::Normal,
                },
                GaugeZone {
                    start: warning,
                    end: critical,
                    severity: ZoneSeverity::Warning,
                },
                GaugeZone {
                    start: critical,
                    end: limit.max(critical),
                    severity: ZoneSeverity::Critical,
                },
            ],
        }
    }

    pub fn value_to_angle(&self, value: f64) -> f64 {
        let span = (self.max - self.min).max(f64::EPSILON);
        let ratio = ((value - self.min) / span).clamp(0.0, 1.0);
        self.start_angle + ratio * (self.end_angle - self.start_angle)
    }

    pub fn angle_to_value(&self, angle: f64) -> f64 {
        let span = (self.end_angle - self.start_angle).max(f64::EPSILON);
        let ratio = ((angle - self.start_angle) / span).clamp(0.0, 1.0);
        self.min + ratio * (self.max - self.min)
    }

    pub fn severity_at(&self, value: f64) -> ZoneSeverity {
        self.zones
            .iter()
            .find(|zone| value >= zone.start && value <= zone.end)
            .map(|zone| zone.severity)
            .unwrap_or(ZoneSeverity::Normal)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NeedleState {
    pub current_angle: f64,
    pub target_angle: f64,
    pub velocity: f64,
}

impl NeedleState {
    /// Advances a stable, critically damped instrument needle. `dt_seconds`
    /// is clamped so delayed terminal redraws cannot cause a jump.
    pub fn advance(&mut self, target_angle: f64, dt_seconds: f64, responsiveness: f64) {
        self.target_angle = target_angle;
        let dt = dt_seconds.clamp(0.0, 0.1);
        let omega = responsiveness.max(1.0);
        let acceleration =
            omega * omega * (target_angle - self.current_angle) - 2.0 * omega * self.velocity;
        self.velocity += acceleration * dt;
        self.current_angle += self.velocity * dt;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PeakHold {
    pub value: f64,
}

impl PeakHold {
    pub fn observe(&mut self, value: f64, decay_per_second: f64, dt_seconds: f64) {
        self.value = self.value.max(value);
        self.value = (self.value - decay_per_second.max(0.0) * dt_seconds.max(0.0)).max(value);
    }

    pub fn reset(&mut self) {
        self.value = 0.0;
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    Off,
    Starting,
    Idle,
    Cruising,
    Accelerating,
    FullThrottle,
    Warning,
    Fault,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryFreshness {
    Live,
    Stale,
    Offline,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TelemetryEvent {
    pub timestamp: DateTime<Utc>,
    pub rpm: Option<f64>,
    pub tpm: Option<f64>,
    pub rpd: Option<f64>,
    pub lifetime_input: Option<u64>,
    pub lifetime_output: Option<u64>,
    pub latency: Option<f64>,
    pub provider: String,
    pub status: TelemetryFreshness,
}

impl TelemetryEvent {
    pub fn lifetime_tokens(&self) -> Option<u64> {
        self.lifetime_input
            .zip(self.lifetime_output)
            .map(|(input, output)| input.saturating_add(output))
            .or(self.lifetime_input)
            .or(self.lifetime_output)
    }

    pub fn engine_state(&self, rpm_max: f64, tpm_max: f64, rpd_warning: f64) -> EngineState {
        if self.status == TelemetryFreshness::Offline {
            return EngineState::Fault;
        }
        let rpm = self.rpm.unwrap_or(0.0);
        let tpm = self.tpm.unwrap_or(0.0);
        let rpd = self.rpd.unwrap_or(0.0);
        if rpd >= rpd_warning || self.status == TelemetryFreshness::Stale {
            return EngineState::Warning;
        }
        if rpm >= rpm_max * 0.9 || tpm >= tpm_max * 0.9 {
            EngineState::FullThrottle
        } else if rpm > 0.0 && tpm > 0.0 {
            EngineState::Cruising
        } else {
            EngineState::Idle
        }
    }
}

/// Splits an odometer value into fixed-width mechanical digit wheels.
pub fn odometer_digits(value: u64, width: usize) -> Vec<u8> {
    format!("{value:0width$}", width = width)
        .bytes()
        .map(|digit| digit - b'0')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibrated_geometry_round_trips_and_clamps() {
        let gauge = GaugeConfig::tachometer(10_000.0, 8_500.0);
        assert_eq!(gauge.value_to_angle(0.0), -135.0);
        assert_eq!(gauge.value_to_angle(10_000.0), 135.0);
        assert_eq!(gauge.angle_to_value(0.0), 5_000.0);
        assert_eq!(gauge.value_to_angle(99_999.0), 135.0);
    }

    #[test]
    fn zones_and_peaks_follow_boundaries() {
        let gauge = GaugeConfig::tachometer(10_000.0, 8_500.0);
        assert_eq!(gauge.severity_at(8_600.0), ZoneSeverity::Redline);
        let mut peak = PeakHold::default();
        peak.observe(90.0, 1.0, 0.0);
        peak.observe(50.0, 1.0, 2.0);
        assert_eq!(peak.value, 88.0);
    }

    #[test]
    fn odometer_is_fixed_width_and_saturating() {
        assert_eq!(
            odometer_digits(1_284_739, 10),
            vec![0, 0, 0, 1, 2, 8, 4, 7, 3, 9]
        );
        let event = TelemetryEvent {
            timestamp: Utc::now(),
            rpm: None,
            tpm: None,
            rpd: None,
            lifetime_input: Some(u64::MAX),
            lifetime_output: Some(1),
            latency: None,
            provider: "p".into(),
            status: TelemetryFreshness::Live,
        };
        assert_eq!(event.lifetime_tokens(), Some(u64::MAX));
    }
}
