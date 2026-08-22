// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use chrono::{Duration, Utc};
use ingauge::{
    capacity::{snapshots_at, CapacityPolicy},
    forecast::regression_rate,
    *,
};
use std::{hint::black_box, time::Instant};

fn main() {
    let now = Utc::now();
    let mut observations = Vec::with_capacity(10_000);
    for index in 0..10_000_u64 {
        observations.push(Observation {
            provider: format!("provider-{}", index % 100).into(),
            model: Some(format!("model-{}", index % 10).into()),
            metric: Metric::RemainingTokens,
            value: MetricValue::Integer(100_000 - index),
            observed_at: now,
            source: ObservationSource::Fixture,
            confidence: Confidence::High,
        });
    }
    let started = Instant::now();
    for _ in 0..100 {
        black_box(snapshots_at(
            black_box(&observations),
            now,
            CapacityPolicy::default(),
        ));
    }
    let capacity_ms = started.elapsed().as_secs_f64() * 1_000.0 / 100.0;

    let samples: Vec<_> = (0..10_000_u64)
        .map(|index| (now + Duration::seconds(index as i64), index * 10))
        .collect();
    let started = Instant::now();
    for _ in 0..100 {
        black_box(regression_rate(black_box(&samples), 5));
    }
    let forecast_ms = started.elapsed().as_secs_f64() * 1_000.0 / 100.0;
    println!("{{\"capacity_10k_ms\":{capacity_ms:.3},\"forecast_10k_ms\":{forecast_ms:.3}}}");
}
