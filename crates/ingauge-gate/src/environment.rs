// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT

use std::time::Duration;

const DEFAULT_TIMEOUT_SECONDS: u64 = 5;

pub(crate) fn admit_timeout() -> Duration {
    match std::env::var("INGAUGE_ADMIT_TIMEOUT") {
        Ok(value) => match value.parse::<u64>() {
            Ok(seconds) => Duration::from_secs(seconds),
            Err(error) => {
                tracing::warn!(event = "invalid_admit_timeout", value = %value, %error, "using default admission timeout");
                Duration::from_secs(DEFAULT_TIMEOUT_SECONDS)
            }
        },
        Err(_) => Duration::from_secs(DEFAULT_TIMEOUT_SECONDS),
    }
}
