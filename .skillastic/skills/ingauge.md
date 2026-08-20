---
name: ingauge
description: Operate, configure, test, and extend the InGauge Rust inference-capacity telemetry system. Use when Codex needs to inspect provider headroom, probe Harness usage, validate InGauge TOML, read JSON capacity output, work with SQLite observations, add provider adapters or canonical metrics, change forecasting and capacity-state logic, or diagnose the ingauge CLI and daemon.
---

# InGauge

Treat InGauge as a provider-neutral capacity observability system. Keep provider-specific authentication, endpoints, response shapes, and semantics inside adapters; expose only canonical observations to the core.

## Locate and inspect

Work from the repository containing `Cargo.toml` with `package.name = "ingauge"`. Inspect `README.md`, `ingauge.example.toml`, and the relevant module before changing behavior.

Key modules:

- `src/model.rs`: canonical IDs, metrics, observations, quotas, capacity states, and events.
- `src/providers.rs`: adapter trait, Harness implementation, and structured provider errors.
- `src/capacity.rs`: observation-to-capacity derivation.
- `src/forecast.rs`: rates and ordered capacity events.
- `src/store.rs`: SQLite schema and observation persistence.
- `src/config.rs`: TOML loading and endpoint validation.
- `src/main.rs`: CLI dispatch and presentation.

## Operate

Prefer the built binary when available; otherwise use Cargo offline after dependencies have been fetched.

```bash
ingauge
ingauge --json
ingauge --config ingauge.toml
ingauge config validate --config ingauge.toml
ingauge next
```

Interpret provider failures independently. Distinguish authentication, rate limiting, network failure, timeout, invalid response, unsupported behavior, and unavailable capacity. Never turn an unknown measurement into zero capacity.

For automation, consume `--json`; do not scrape terminal tables. Preserve raw quota values and reset timestamps instead of reducing observations to percentages.

## Configure safely

Use TOML references to credentials, such as `api_key_env`, and resolve secrets at runtime. Never place API keys in configuration, SQLite, logs, errors, fixtures, JSON output, or skill files.

Require HTTPS for remote provider endpoints. Permit plain HTTP only for explicit loopback services. Validate configuration before probing or starting continuous operation.

## Add or change an adapter

1. Implement `ProviderAdapter` and keep transport/parsing types private to the adapter.
2. Map provider records into canonical `Observation` values with provider, source, observation time, and confidence.
3. Preserve simultaneous quotas; do not select one provider-specific limit prematurely.
4. Convert failures into `ProviderError` without leaking request headers or credentials.
5. Add fixture-driven parser tests for valid, partial, malformed, authentication, rate-limit, reset, and multiple-quota responses.
6. Confirm one failed adapter does not suppress successful providers.

Do not add provider names or response structures to `model.rs`, `capacity.rs`, `forecast.rs`, or `store.rs`.

## Change forecasting

Derive rates from ordered historical observations. Avoid forecasting from fewer than the configured minimum sample count. Calculate exhaustion only for positive consumption rates, compare it with reset time, and emit ordered `ProjectedExhaustion` and `QuotaReset` events.

Keep state classification independent from terminal colour and symbols. The tightest applicable quota determines effective headroom.

## Verify

Run the deterministic local gate before claiming completion:

```bash
cargo fmt --all -- --check
cargo test --offline
deliver --spec deliver.toml --strict
```

Use fixture data rather than live provider APIs in tests. Check `git diff` for credentials and unintended provider-specific coupling before committing.
