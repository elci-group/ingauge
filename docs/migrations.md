# Storage and configuration migrations

Configuration schema version 1 is the only supported format. Unknown fields and unsupported versions fail before network or storage access. Credentials remain environment references.

SQLite schema versions use `PRAGMA user_version` and are independent from the package version. Opening a writable store applies forward migrations inside a transaction. A database newer than the binary is rejected. Before upgrading, run `ingauge db integrity`, create `ingauge db backup PATH`, stop the daemon, install the binary, and run `ingauge db migrate`. Validate with `ingauge db integrity` and `ingauge health` before removing the backup.

Version 1 retains the MVP observation columns, adds deterministic indexes, probe-run history, daemon heartbeat state, an application identifier, WAL configuration, and typed round-trip validation. Rolling back to 0.1 is unsupported after the daemon has written version-1 operational state; restore the pre-upgrade backup instead.
