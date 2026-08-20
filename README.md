# InGauge

Rust-native, read-only inference-capacity observability for local routing infrastructure.

## Quick start

```bash
cargo run -- --json
cargo run -- config validate --config ingauge.toml
```

The initial adapter is Harness. It expects `GET {endpoint}/usage` returning an array of records:

```json
[{"provider":"groq","model":"GPT-OSS-120B","used":88000,"limit":100000,"remaining":12000,"reset_at":"2026-08-20T20:49:00Z","tokens_per_minute":400}]
```

Provider errors are isolated, credentials are never persisted, and observations can be stored in SQLite with `general.database`.
