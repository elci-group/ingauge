# InGauge v0.2 improvement ledger

This ledger groups one hundred independently verifiable production-core improvements. A checked item is backed by source, tests, documentation, or a measured artifact in this repository.

## Configuration and contracts (1–10)

- [x] 001 Version the package and machine contract.
- [x] 002 Declare and enforce an MSRV.
- [x] 003 Reject unknown top-level configuration fields.
- [x] 004 Reject unknown general configuration fields.
- [x] 005 Reject unknown provider configuration fields.
- [x] 006 Reject unknown forecasting fields.
- [x] 007 Parse bounded polling durations.
- [x] 008 Parse bounded retention durations.
- [x] 009 Resolve configuration through explicit, environment, and XDG paths.
- [x] 010 Validate enabled provider endpoints and credential references.

## Capacity model (11–22)

- [x] 011 Validate provider identifiers.
- [x] 012 Validate model identifiers.
- [x] 013 Preserve fractional metric values.
- [x] 014 Distinguish unknown capacity from zero capacity.
- [x] 015 Group observations deterministically.
- [x] 016 Inject snapshot evaluation time.
- [x] 017 Derive remaining quota without underflow.
- [x] 018 Preserve simultaneous quota types.
- [x] 019 Select the tightest effective headroom.
- [x] 020 Make state thresholds configurable.
- [x] 021 Reject non-finite rates.
- [x] 022 Order snapshots by provider and model.

## Harness adapter (23–32)

- [x] 023 Separate Harness parsing from transport.
- [x] 024 Reuse a configured HTTP client.
- [x] 025 Enforce connection timeout.
- [x] 026 Enforce request timeout.
- [x] 027 Bound response body bytes.
- [x] 028 Bound response record count.
- [x] 029 Map authentication failures.
- [x] 030 Map rate-limit failures.
- [x] 031 Map transient network and timeout failures.
- [x] 032 Preserve partial and multi-model records.

## SQLite storage (33–46)

- [x] 033 Bundle a SQLite release containing the WAL-reset fix.
- [x] 034 Version the database schema independently.
- [x] 035 Run migrations transactionally.
- [x] 036 Set a SQLite application identifier.
- [x] 037 Enable WAL for local concurrent reads.
- [x] 038 Set synchronous NORMAL explicitly.
- [x] 039 Set a bounded busy timeout.
- [x] 040 Add provider/model/time and metric/time indexes.
- [x] 041 Batch observations transactionally.
- [x] 042 Read observations back as canonical typed values.
- [x] 043 Query bounded ordered history.
- [x] 044 Query the latest observation set.
- [x] 045 Prune history by retention cutoff.
- [x] 046 Expose integrity, checkpoint, backup, and heartbeat operations.

## Forecasting (47–56)

- [x] 047 Sort rate samples by timestamp.
- [x] 048 Require the configured minimum sample count.
- [x] 049 Ignore zero-duration windows.
- [x] 050 Segment counter resets.
- [x] 051 Compute deterministic least-squares rates.
- [x] 052 Reject non-positive consumption forecasts.
- [x] 053 Bound exhaustion by reset time.
- [x] 054 Report sample count and forecast window.
- [x] 055 Report forecast confidence.
- [x] 056 Order reset, exhaustion, and recovery events.

## CLI (57–66)

- [x] 057 Implement status dispatch.
- [x] 058 Implement provider listing.
- [x] 059 Implement live probe.
- [x] 060 Implement bounded history queries.
- [x] 061 Implement forecast output.
- [x] 062 Implement next-event output.
- [x] 063 Implement daemon health output.
- [x] 064 Implement database administration commands.
- [x] 065 Emit a stable JSON envelope.
- [x] 066 Use stable categorized process exit codes.

## Daemon and Linux operation (67–78)

- [x] 067 Probe immediately on daemon start.
- [x] 068 Delay rather than burst missed interval ticks.
- [x] 069 Prevent overlapping poll cycles.
- [x] 070 Persist daemon heartbeat state.
- [x] 071 Track last successful poll.
- [x] 072 Prune retention periodically.
- [x] 073 Apply bounded transient backoff.
- [x] 074 Handle SIGINT gracefully.
- [x] 075 Handle SIGTERM gracefully on Unix.
- [x] 076 Reload safe configuration on SIGHUP.
- [x] 077 Reject live database-path changes.
- [x] 078 Ship a hardened systemd service definition.

## Security and observability (79–86)

- [x] 079 Remove production unwrap and expect calls.
- [x] 080 Use typed secret-safe application errors.
- [x] 081 Emit structured poll and provider spans.
- [x] 082 Support environment-controlled filtering.
- [x] 083 Support deterministic non-ANSI logs.
- [x] 084 Never persist credentials.
- [x] 085 Document a scoped threat model.
- [x] 086 Document recovery and residual risks.

## Testing and supply chain (87–94)

- [x] 087 Move orchestration into testable library modules.
- [x] 088 Add deterministic capacity tests.
- [x] 089 Add parser boundary tests.
- [x] 090 Add migration and round-trip tests.
- [x] 091 Add forecast characterization tests.
- [x] 092 Deny warnings across all targets.
- [x] 093 Forbid unsafe code and placeholder macros.
- [x] 094 Gate documentation and release builds.

## Performance, Padagonia, and release (95–100)

- [x] 095 Add deterministic core benchmarks.
- [x] 096 Tune the release profile with thin LTO.
- [x] 097 Export deterministic Padagonia-compatible projections.
- [x] 098 Document operations and migration behavior.
- [x] 099 Maintain a strict Deliver acceptance contract.
- [x] 100 Route versioning and commits through Kaptaind.
