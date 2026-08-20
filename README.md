# InGauge — know your inference headroom

InGauge makes inference capacity visible, predictable, and operational before workloads hit a limit. The Rust-native daemon probes any provider, router, or harness through a bounded canonical bridge, stores typed history in SQLite, derives headroom, and forecasts consumption from ordered samples.

## Quick start

```bash
cp ingauge.example.toml ingauge.toml
cargo run -- --config ingauge.toml config validate
cargo run -- --config ingauge.toml probe --json
cargo run -- --config ingauge.toml daemon
```

Commands include `status`, `providers`, `discover`, `probe`, `history`, `forecast`, `next`, `health`, `daemon`, `config`, `db`, and `export padagonia`. `discover` inventories known local provider credentials, router binaries, and every ViCo harness manifest without reading or returning secret values. Automation should consume `--json`, whose envelope is versioned independently from individual command data.

Human output uses an instrument-panel layout with semantic colour, contextual emoji, and a brief gauge animation on interactive terminals. Pipes stay ANSI-free, `NO_COLOR=1` disables colour, `INGAUGE_NO_ANIMATION=1` disables motion, and `--json` remains decoration-free for automation.

Each configured target expects `GET {endpoint}{usage_path}` (default `/usage`) returning an array:

```json
[{"provider":"groq","model":"GPT-OSS-120B","used":88000,"limit":100000,"remaining":12000,"reset_at":"2026-08-20T20:49:00Z","tokens_per_minute":400.5}]
```

Remote endpoints require HTTPS; loopback Harness endpoints may use HTTP. Set `api_key_env` to the name of a bearer-token environment variable. The token is read only at request time and is never persisted.

Configuration keys are not restricted to a vendor allowlist, so the same contract supports the locally detected ViCo, GIA, Hermes, Ollama, Codex, Antigravity, Crush, Gemini, Groq, Kimi, OpenClaw, HyperAgent, and future harnesses. Vendor APIs that do not expose a read-only quota endpoint should be connected through a local or remote canonical bridge; InGauge deliberately does not make billable inference calls merely to estimate capacity.

## Operations

The daemon probes immediately and then on a delay-based interval, writes heartbeat state, applies retention, and shuts down cleanly on SIGINT or SIGTERM. SIGHUP atomically reloads valid configuration but rejects a database-path change. `health` reports heartbeat freshness, while `db integrity`, `db checkpoint`, and `db backup` support maintenance and recovery.

Install the binary, `packaging/ingauge.service`, man page, completion file, and a version-1 configuration under `/etc/ingauge`. SQLite WAL requires a local filesystem; copy the database through `db backup`, not by separating an active database from its WAL/SHM files.

Padagonia export is optional:

```bash
ingauge --config ingauge.toml export padagonia --output capacity-graph.json --since 24h
```

It emits a deterministic Padagonia projection-shaped graph without adding Padagonia as a runtime dependency.

## Quality and performance

```bash
scripts/ci.sh
cargo bench --bench core --offline
deliver --spec deliver.toml --strict
```

See `docs/improvements-v0.2.md`, `docs/threat-model.md`, and `docs/migrations.md` for acceptance evidence and operational boundaries.
