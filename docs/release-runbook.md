# Release and rollback runbook

<p align="center">
  <strong>🚢 Qualify → commit → push → ship → verify</strong><br>
  <sub>🟢 fail closed · 🔵 preserve evidence · 🟠 rollback deliberately</sub>
</p>

## Preconditions

- 🧭 Work from `main` with the expected `origin` and an active Kaptaind Aim-of-Change.
- 🔒 Keep credentials in environment variables; never place tokens or signing material in repository files.
- ✅ Require a clean, connected runner for advisory refresh, release publication, and remote verification.
- 📦 Confirm the target list and distribution channels with `kaptaind-cli ship plan --format json`.

## Qualified release

```bash
poka plan
scripts/ci.sh
cargo deny check --hide-inclusion-graph
brandi lint --path . --strict --fail-under 90
deliver --spec deliver.toml --strict
kaptaind-cli analyze
kaptaind-cli ship plan --format json
```

Kaptaind owns semantic versioning and commits. With `[push] enabled = true`, a qualifying cluster is committed and pushed only after `scripts/ci.sh` succeeds. Force pushes remain disabled, the upstream must already exist, and the configured `main` target is explicit.

After the remote branch matches the local commit, publish through the configured release pipeline:

```bash
kaptaind-cli ship run
kaptaind-cli ship status --format json
```

## Acceptance checks

- 🟢 The release command exits successfully and the ship status records the intended version, commit, target, and channel.
- 🟢 Every binary matches its published SHA-256 digest.
- 🟢 The SPDX SBOM and SLSA provenance list the same artifact digests.
- 🟢 A clean temporary host can unpack the binary and run `ingauge --version` plus `ingauge config validate`.
- 🟢 A fixture bridge can be probed without a billable inference call.
- 🟢 The service starts, writes a heartbeat, passes `ingauge health`, and stops cleanly.

## Rollback decision

Rollback when an installed artifact fails integrity, configuration compatibility, startup, health, fixture probing, or data restoration. Do not rewrite a published tag or force-push `main`.

1. Stop the service and retain the failed binary, logs, configuration, database, WAL, and SHM as incident evidence.
2. Run `ingauge db integrity` and create a new backup if the database is readable.
3. Reinstall the last known-good signed artifact from the release index.
4. If the newer binary wrote an incompatible schema, restore the pre-upgrade backup as one recovery unit.
5. Start the service, validate configuration, probe the fixture, and verify heartbeat freshness.
6. Record the failed release, observed symptoms, rollback artifact digest, database action, and recovery time.
7. Fix forward on a new version. Never reuse the withdrawn version or mutate its artifacts.

## Evidence bundle

Retain the commit SHA, Kaptaind analysis, Deliver report, Cargo advisory result, benchmark output, ship plan/status, checksums, SBOM, provenance, install transcript, health result, and rollback drill. A release is complete only when these records agree on version and artifact identity.
