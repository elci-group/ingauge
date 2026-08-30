# InGauge — know your inference headroom

InGauge makes inference capacity visible, predictable, and operational before workloads hit a limit. The Rust-native daemon probes any provider, router, or harness through a bounded canonical bridge, stores typed history in SQLite, derives headroom, and forecasts consumption from ordered samples.

## Quick start

```bash
cp ingauge.example.toml ingauge.toml
cargo run -- --config ingauge.toml config validate
cargo run -- --config ingauge.toml config tui
cargo run -- --config ingauge.toml probe --json
cargo run -- --config ingauge.toml daemon
```

Commands include `status`, `providers`, `discover`, `probe`, `history`, `forecast`, `next`, `health`, `daemon`, `config`, `db`, and `export padagonia`. `discover` inventories known local provider credentials, router binaries, and every ViCo harness manifest without reading or returning secret values. Automation should consume `--json`, whose envelope is versioned independently from individual command data.

Human output uses a persistent four-instrument motorsport cockpit with semantic colour, wood-grain trim, calibration and consumption rails. On an interactive terminal, `ingauge` enters an alternate-screen live render loop until `Ctrl+C`: a short ignition sweep runs first, critically damped needles advance at 12.5 FPS, telemetry is sampled independently, and provider dial sets rotate without interrupting rendering. Its primary instruments are side-by-side rasterized circular faces with bezels, angular tick marks, a continuous redline arc, centre hubs, and value-driven needles; the resource and odometer instruments sit beneath them like a physical dashboard. `REVS` is inference activity in requests per minute, `SPEED` is the canonical tokens-per-minute (TPM) throughput, `OIL / RPD` is daily resource consumption, and `LIFETIME MILEAGE` is a fixed-width token odometer. OpenAI, Anthropic, Groq, Gemini, and OpenRouter each have a distinct instrument palette and provider badge; unknown bridges receive a deterministic palette. The reusable `instrument` module keeps calibrated gauge geometry, zones, needle physics, peak hold, freshness, and engine-state derivation separate from collection and rendering. Pipes stay one-shot and ANSI-free, `NO_COLOR=1` disables colour, `INGAUGE_NO_ANIMATION=1` disables the brief non-dashboard command animation, and `--json` remains one-shot and decoration-free for automation.

`ingauge config tui` opens the provider configuration garage. A plain interactive `ingauge` or `ingauge status` launch opens the same garage automatically when no enabled provider is configured; piped commands, explicit `--json` output, and other subcommands never prompt. Presets are provided for OpenAI, Anthropic, Groq, Gemini, and OpenRouter, plus a custom canonical bridge. Credentials may come from a `.bashrc` export, a selected `.env`, or masked manual entry. Manual keys are written to a mode-0600 `secrets.env` under the user's Ingauge configuration directory, outside the project tree; raw keys are never written to `ingauge.toml`, JSON output, or logs.

The Groq preset connects directly to `https://api.groq.com/openai/v1/models`. Ingauge converts Groq's authenticated rolling token-limit headers into TPM utilisation and its daily request-limit headers into RPD resource health; when those headers are unavailable, a successful request reports a connected idle instrument rather than inventing usage. The other presets currently target the canonical telemetry bridge because their public APIs do not share one honest usage contract. Ordinary `status` renders bridge/API failures as a faulted cockpit; explicit `probe` remains strict and exits unsuccessfully when no target can be read.

On Linux, the interactive cockpit also detects provider API activity from encrypted network traffic. The collector resolves each configured provider endpoint, samples user-visible TCP counters with `ss -tinp`, excludes Ingauge's own sockets, and tracks byte deltas across both short-lived and persistent TLS connections. Outbound bursts drive estimated RPM; rolling inbound bytes drive estimated `TPM~`. No packet payload, prompt, response, API key, or process environment is read. Because TLS hides request boundaries and token counts—and CDN addresses can be shared—network readings are explicitly labelled `NET EST` and never replace a positive authoritative provider rate. Exact token and lifetime mileage telemetry still requires a provider usage API, canonical bridge, or application instrumentation.

Gauge calibration belongs in the optional `[instruments]` configuration. `provider_cycle_seconds` controls provider dial rotation from 1 to 60 seconds and defaults to 4. `dashboard_sample_seconds` controls independent telemetry sampling from 2 to 300 seconds and defaults to 15, avoiding excessive calls to remote provider APIs while the dials continue rendering smoothly. RPM and TPM support `fixed`, `adaptive`, `historical`, and `provider-specific` scale modes; the terminal renderer currently presents fixed calibration while retaining the mode for richer clients. RPD is consumption-oriented: below `warning` is healthy, then warning, then critical at `critical`.

`[instruments.network]` controls encrypted traffic detection. It is enabled by default, samples every 250 ms, and estimates one token per four received bytes. Set `enabled = false` to disable process/socket inspection, or tune `bytes_per_token` from 1–32 for a workload-specific estimate. If `ss` is unavailable or socket inspection is denied, the provider/API telemetry path continues normally.

Each configured target expects `GET {endpoint}{usage_path}` (default `/usage`) returning an array:

```json
[{"provider":"groq","model":"GPT-OSS-120B","used":88000,"limit":100000,"remaining":12000,"reset_at":"2026-08-20T20:49:00Z","tokens_per_minute":400.5,"requests_per_minute":30,"output_tokens":1200,"daily_usage":450,"daily_limit":1000}]
```

Remote endpoints require HTTPS; loopback Harness endpoints may use HTTP. Set `api_key_env` to the name of a bearer-token environment variable. The token is read only at request time and is never persisted.

The optional bridge telemetry fields are `monthly_usage`/`monthly_limit` (or `budget`/`budget_limit`), `output_tokens`, `requests`/`request_limit`, `tokens_per_minute`, `requests_per_minute`, and `daily_usage`/`daily_limit`. Monthly quota wins over budget for the fuel gauge; token usage and limits remain available as the fallback quota.

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
brandi lint --strict --fail-under 90
cargo bench --bench core --offline
deliver --spec deliver.toml --strict
```

## Project state

InGauge currently grades **85/100 (A−)** across ten equally weighted production criteria: production-ready, with release trust, resilience evidence, performance budgets, and operational SLOs remaining before a SOTA claim.

- 📊 Read the evidence-backed [SOTA assessment](docs/sota-assessment.md).
- 🎯 Follow the measurable [roadmap to SOTA](ROADMAP.md).
- 🧾 Review the [v0.2 improvement ledger](docs/improvements-v0.2.md).
- 🚢 Use the qualification-gated [release and rollback runbook](docs/release-runbook.md).
- 🔐 Operate within the documented [threat model](docs/threat-model.md) and [migration runbook](docs/migrations.md).
