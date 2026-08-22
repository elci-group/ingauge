# InGauge roadmap

<p align="center">
  <strong>🎯 Target: ≥95/100 with reproducible evidence</strong><br>
  <sub>🔴 P0 release trust → 🟠 P1 resilience → 🔵 P2 scale → 🟢 P3 ecosystem</sub>
</p>

> **Scope note:** “SOTA” is an evidence threshold, not a feature count. A milestone closes only when its command, artifact, or measured result is repeatable in CI and linked from a release qualification report.

## Flight path

```mermaid
flowchart LR
    P0[🔴 P0 · 0–2 weeks<br/>Trust the release] --> P1[🟠 P1 · 2–6 weeks<br/>Prove resilience]
    P1 --> P2[🔵 P2 · 6–10 weeks<br/>Measure scale]
    P2 --> P3[🟢 P3 · 10–14 weeks<br/>Lead the category]
    classDef red fill:#101820,stroke:#FF4D6D,color:#F4FAFD,stroke-width:2px;
    classDef amber fill:#101820,stroke:#FFB020,color:#F4FAFD,stroke-width:2px;
    classDef cyan fill:#101820,stroke:#58D5FF,color:#F4FAFD,stroke-width:2px;
    classDef green fill:#101820,stroke:#41D38A,color:#F4FAFD,stroke-width:2px;
    class P0 red;
    class P1 amber;
    class P2 cyan;
    class P3 green;
```

## 🔴 P0 — Trust the release (0–2 weeks)

Goal: every published byte is built, tested, attributable, and recoverable.

- [ ] Run locked CI on Linux and macOS for the MSRV and stable Rust; require `fmt`, Clippy, tests, rustdoc, release build, and config validation.
- [ ] Run `cargo deny` with a refreshed RustSec database and retain the report as release evidence.
- [ ] Generate SPDX SBOM and SLSA provenance through Kaptaind; sign tags, checksums, binaries, and attestations.
- [ ] Protect `main` with required CI/security checks and review; pin third-party Actions by commit digest.
- [ ] Publish `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` archives with `SHA256SUMS`.
- [ ] Exercise clean install, systemd start, config validation, upgrade, backup, restore, rollback, and uninstall in disposable runners.
- [ ] Add `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`, and a release/rollback runbook.

Exit evidence:

- 🟢 100% of release artifacts have matching checksum, SBOM subject, and provenance subject.
- 🟢 A clean runner can install, probe a fixture, upgrade, restore, and uninstall without manual repair.
- 🟢 No unresolved high/critical advisory; exceptions are time-bounded and documented.

## 🟠 P1 — Prove resilience (2–6 weeks)

Goal: expected failures are characterized, bounded, and observable.

- [ ] Reach ≥90% line and ≥85% branch coverage in production modules.
- [ ] Add fuzz targets for provider JSON, configuration/duration parsing, identifiers, and migration inputs.
- [ ] Add mutation testing with ≥80% mutation score for capacity, forecast, admission, and storage logic.
- [ ] Run Miri on pure/core modules and sanitizers on integration fixtures.
- [ ] Build fault-injection tests for timeout storms, partial bodies, corrupt rows, WAL recovery, disk-full behavior, clock skew, and process termination during migration.
- [ ] Split validation policy from `src/config.rs`; extract repeated admission decisions and model validation to take every Fract warning below 0.65 entropy.
- [ ] Prototype replacing `thiserror` first; replace `chrono` only if the resulting API and timestamp correctness remain simpler and all characterization tests pass.

Exit evidence:

- 🟢 Zero critical/warning Fract modules, no cycles, architecture score ≥95/100.
- 🟢 A 24-hour fault-injected soak has no deadlock, unbounded growth, credential leak, or data-integrity loss.
- 🟢 All fuzz targets complete 10 million cases or 24 hours without a crash.

## 🔵 P2 — Measure scale and operations (6–10 weeks)

Goal: publish capacity envelopes and operate against explicit objectives.

- [ ] Define SLOs: probe success ≥99.9%, p95 loopback probe <100 ms excluding upstream latency, admission overhead p95 <10 ms, recovery point objective ≤poll interval.
- [ ] Export OpenMetrics for probe latency/errors, freshness, headroom, forecast confidence, database size, prune count, retry count, and admission decisions.
- [ ] Add dashboards and burn-rate alerts with low-cardinality provider/model labels.
- [ ] Benchmark 10, 100, and 1,000 targets; 1 million and 100 million observations; concurrent readers; retention pruning; and backup under load.
- [ ] Enforce performance budgets in CI: no >5% median regression over a noise-qualified baseline and no unbounded RSS growth.
- [ ] Propagate a probe/run correlation ID through transport, storage, forecasts, exports, and admission decisions.
- [ ] Continuously restore the newest backup and compare schema/application IDs plus sampled records.

Exit evidence:

- 🟢 Published benchmark bundle includes host profile, raw samples, confidence intervals, and commit SHA.
- 🟢 SLO dashboard and alert tests cover healthy, stale, degraded, and exhausted states.
- 🟢 Restore drill meets documented RPO/RTO with hash-checked evidence.

## 🟢 P3 — Lead the category (10–14 weeks)

Goal: make InGauge the reference interoperability layer for inference headroom.

- [ ] Publish the canonical capacity endpoint as a versioned JSON Schema with conformance fixtures.
- [ ] Add a provider/bridge certification suite and compatibility matrix covering auth, pagination, reset semantics, partial data, and rate dimensions.
- [ ] Stabilize `ingauge-gate` with semantic compatibility guarantees, styled accessible timer output, and language-neutral HTTP examples.
- [ ] Add signed deterministic exports and optional authenticated provenance without weakening the single-host default.
- [ ] Publish architecture decision records for storage, timing, bridge boundaries, privacy, and forecasting assumptions.
- [ ] Recruit three independent production adopters and publish anonymized accuracy, false-alarm, and operator-response outcomes.
- [ ] Run an external security review and close all high-severity findings before declaring SOTA.

Exit evidence:

- 🟢 ≥3 independently implemented bridges pass the same conformance suite.
- 🟢 Forecast calibration and error bands are published against real anonymized workloads.
- 🟢 Independent review plus the ten-criterion reassessment scores ≥95/100 with no criterion below 9/10.

## Continuous score gate

| Signal | Current | SOTA gate |
|---|---:|---:|
| 📐 Fract architecture health | 88.9/100 | ≥95/100 |
| 🎨 Brandi coherence | 100/100 | 100/100 |
| 🧪 Production line coverage | Not yet reported | ≥90% |
| 🧬 Mutation score | Not yet reported | ≥80% |
| ⚡ Performance regression | Bench exists; no budget | ≤5% |
| 🔐 Signed SBOM + provenance | Configured; release evidence pending | 100% of artifacts |
| 🛰️ SLO attainment | Not yet defined | ≥99.9% probe success |
| 🚢 Clean install/upgrade/restore | Manual docs | Automated on every release |

## Decision rules

- 🚫 Never trade the stable JSON envelope for visual decoration.
- ♿ Honour `NO_COLOR`, non-TTY output, `TERM=dumb`, CI, and reduced-motion settings.
- 🔒 Never probe by making a billable inference request merely to estimate capacity.
- 📏 Do not merge a performance claim without raw samples and an environment fingerprint.
- 🧾 Do not publish an artifact that is absent from checksums, SBOM, and provenance.
- 🧭 Regrade all ten criteria at the end of each phase; roadmap priority follows the lowest score.
