#!/bin/sh
# Copyright (c) 2026 sal
# SPDX-License-Identifier: MIT
set -eu

cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features --offline -- -D warnings
cargo test --locked --all-targets --offline
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --offline
cargo build --locked --release --offline
