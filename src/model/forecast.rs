// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use super::Confidence;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ForecastResult {
    pub rate_per_minute: f64,
    pub samples: usize,
    pub window_seconds: i64,
    pub confidence: Confidence,
}
