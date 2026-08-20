# Threat model

InGauge is a single-host observer. It trusts the host kernel and operator, but treats provider responses, configuration, environment variables, database files, timestamps, and command arguments as untrusted.

Protected assets are provider credentials, observation integrity, forecast correctness, history availability, logs, backups, and release artifacts. Provider payloads are bounded before parsing; endpoints require HTTPS except for explicit loopback HTTP; credentials are read at probe time and never written to logs, JSON, SQLite, fixtures, or errors. Structured errors expose categories rather than request headers or URLs containing credentials.

SQLite uses a bundled patched release, WAL, a busy timeout, transactional migrations, integrity checking, and online backup. WAL databases must remain on a local filesystem. WAL, SHM, and database files form one recovery unit while the daemon is active. Backups must be protected with the same permissions as the live database and restore tests must accompany schema changes.

The systemd unit runs without a persistent identity, grants a private state directory, denies privilege escalation, and restricts writable filesystem access. Residual risks include a compromised host, malicious configured endpoint, disk exhaustion outside configured retention, operator replacement of history, and provider data poisoning. InGauge does not claim authenticated provenance, multi-tenant isolation, replication, or cryptographic immutability.
