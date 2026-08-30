// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
use super::clock::epoch_millis;
use super::fields::process_id;
use super::sequence::next_sequence;

/// Build a stable, process-local correlation identifier for a polling cycle.
pub fn poll_correlation_id() -> String {
    let millis = epoch_millis();
    format!("{}-{millis}-{}", process_id(), next_sequence())
}
