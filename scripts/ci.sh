#!/bin/sh
set -eu

cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features --offline -- -D warnings
cargo test --locked --all-targets --offline
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --offline
cargo build --locked --release --offline
