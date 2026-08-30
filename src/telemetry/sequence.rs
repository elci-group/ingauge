// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT

use std::sync::atomic::{AtomicU64, Ordering};

static POLL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn next_sequence() -> u64 {
    POLL_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}
