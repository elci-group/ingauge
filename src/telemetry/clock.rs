// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT

use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}
