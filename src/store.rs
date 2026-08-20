use crate::model::*;
use chrono::{DateTime, Utc};
use rusqlite::{backup::Backup, params, Connection, OptionalExtension};
use std::{path::Path, time::Duration};

const SCHEMA_VERSION: i64 = 1;
const APPLICATION_ID: i64 = 0x494E4741;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid stored timestamp: {0}")]
    Timestamp(String),
    #[error("unsupported database schema version {0}")]
    UnsupportedSchema(i64),
}

pub struct Store {
    conn: Connection,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DaemonHealth {
    pub heartbeat_at: DateTime<Utc>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub pid: u32,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "application_id", APPLICATION_ID)?;
        let mut store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(Self { conn })
    }

    pub fn migrate(&mut self) -> Result<(), StoreError> {
        let version: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            tracing::error!(
                event = "database_schema_unsupported",
                actual = version,
                supported = SCHEMA_VERSION,
                "database open rejected"
            );
            return Err(StoreError::UnsupportedSchema(version));
        }
        if version == 0 {
            let tx = self.conn.transaction()?;
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS observations (
                    id INTEGER PRIMARY KEY,
                    provider TEXT NOT NULL,
                    model TEXT,
                    metric TEXT NOT NULL,
                    value TEXT NOT NULL,
                    observed_at TEXT NOT NULL,
                    source TEXT NOT NULL,
                    confidence TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS observations_provider_time ON observations(provider, model, observed_at DESC);
                CREATE INDEX IF NOT EXISTS observations_metric_time ON observations(metric, observed_at DESC);
                CREATE TABLE IF NOT EXISTS probe_runs (
                    id INTEGER PRIMARY KEY,
                    provider TEXT NOT NULL,
                    started_at TEXT NOT NULL,
                    completed_at TEXT NOT NULL,
                    success INTEGER NOT NULL,
                    error_code TEXT
                );
                CREATE TABLE IF NOT EXISTS daemon_state (
                    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                    heartbeat_at TEXT NOT NULL,
                    last_success_at TEXT,
                    pid INTEGER NOT NULL
                );" 
            )?;
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn insert(&self, observation: &Observation) -> Result<(), StoreError> {
        self.insert_batch(std::slice::from_ref(observation))
    }

    pub fn insert_batch(&self, observations: &[Observation]) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut statement = tx.prepare_cached(
                "INSERT INTO observations(provider,model,metric,value,observed_at,source,confidence) VALUES(?,?,?,?,?,?,?)"
            )?;
            for observation in observations {
                statement.execute(params![
                    observation.provider.as_str(),
                    observation.model.as_ref().map(ModelId::as_str),
                    serde_json::to_string(&observation.metric)?,
                    serde_json::to_string(&observation.value)?,
                    observation.observed_at.to_rfc3339(),
                    serde_json::to_string(&observation.source)?,
                    serde_json::to_string(&observation.confidence)?,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn history(
        &self,
        provider: Option<&str>,
        model: Option<&str>,
        metric: Option<Metric>,
        since: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<Vec<Observation>, StoreError> {
        let metric = metric
            .map(|value| serde_json::to_string(&value))
            .transpose()?;
        let since = since.map(|value| value.to_rfc3339());
        let mut statement = self.conn.prepare_cached(
            "SELECT provider,model,metric,value,observed_at,source,confidence FROM observations
             WHERE (?1 IS NULL OR provider=?1) AND (?2 IS NULL OR model=?2)
               AND (?3 IS NULL OR metric=?3) AND (?4 IS NULL OR observed_at>=?4)
             ORDER BY observed_at DESC, id DESC LIMIT ?5",
        )?;
        let row_limit = i64::try_from(limit.min(100_000)).unwrap_or(100_000);
        let mut rows = statement.query(params![provider, model, metric, since, row_limit])?;
        let mut output = Vec::new();
        while let Some(row) = rows.next()? {
            output.push(decode_observation(row)?);
        }
        output.reverse();
        Ok(output)
    }

    pub fn latest(&self) -> Result<Vec<Observation>, StoreError> {
        let latest: Option<String> = self
            .conn
            .query_row("SELECT MAX(observed_at) FROM observations", [], |row| {
                row.get(0)
            })
            .optional()?
            .flatten();
        let Some(timestamp) = latest else {
            return Ok(Vec::new());
        };
        let parsed = DateTime::parse_from_rfc3339(&timestamp)
            .map_err(|error| {
                tracing::error!(event = "stored_timestamp_invalid", error = %error, "stored observation timestamp rejected");
                StoreError::Timestamp(timestamp)
            })?
            .with_timezone(&Utc);
        self.history(None, None, None, Some(parsed), 100_000)
    }

    pub fn prune_before(&self, cutoff: DateTime<Utc>) -> Result<usize, StoreError> {
        Ok(self.conn.execute(
            "DELETE FROM observations WHERE observed_at < ?1",
            [cutoff.to_rfc3339()],
        )?)
    }

    pub fn record_probe(
        &self,
        provider: &str,
        started: DateTime<Utc>,
        completed: DateTime<Utc>,
        result: Result<(), &str>,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO probe_runs(provider,started_at,completed_at,success,error_code) VALUES(?1,?2,?3,?4,?5)",
            params![provider, started.to_rfc3339(), completed.to_rfc3339(), result.is_ok(), result.err()],
        )?;
        Ok(())
    }

    pub fn heartbeat(
        &self,
        now: DateTime<Utc>,
        last_success: Option<DateTime<Utc>>,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO daemon_state(singleton,heartbeat_at,last_success_at,pid) VALUES(1,?1,?2,?3)
             ON CONFLICT(singleton) DO UPDATE SET heartbeat_at=excluded.heartbeat_at,last_success_at=COALESCE(excluded.last_success_at,daemon_state.last_success_at),pid=excluded.pid",
            params![now.to_rfc3339(), last_success.map(|value| value.to_rfc3339()), std::process::id()],
        )?;
        Ok(())
    }

    pub fn health(&self) -> Result<Option<DaemonHealth>, StoreError> {
        let row: Option<(String, Option<String>, u32)> = self
            .conn
            .query_row(
                "SELECT heartbeat_at,last_success_at,pid FROM daemon_state WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        row.map(|(heartbeat, success, pid)| {
            let heartbeat = DateTime::parse_from_rfc3339(&heartbeat)
                .map_err(|error| {
                    tracing::error!(event = "stored_timestamp_invalid", field = "heartbeat_at", error = %error, "stored daemon timestamp rejected");
                    StoreError::Timestamp(heartbeat)
                })?
                .with_timezone(&Utc);
            let success = success
                .map(|value| {
                    DateTime::parse_from_rfc3339(&value)
                        .map(|time| time.with_timezone(&Utc))
                        .map_err(|error| {
                            tracing::error!(event = "stored_timestamp_invalid", field = "last_success_at", error = %error, "stored daemon timestamp rejected");
                            StoreError::Timestamp(value)
                        })
                })
                .transpose()?;
            Ok(DaemonHealth {
                heartbeat_at: heartbeat,
                last_success_at: success,
                pid,
            })
        })
        .transpose()
    }

    pub fn integrity_check(&self) -> Result<String, StoreError> {
        Ok(self
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?)
    }

    pub fn checkpoint(&self) -> Result<(), StoreError> {
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA optimize;")?;
        Ok(())
    }

    pub fn backup(&self, destination: impl AsRef<Path>) -> Result<(), StoreError> {
        let mut destination = Connection::open(destination)?;
        let backup = Backup::new(&self.conn, &mut destination)?;
        backup.run_to_completion(128, Duration::from_millis(10), None)?;
        Ok(())
    }
}

fn decode_observation(row: &rusqlite::Row<'_>) -> Result<Observation, StoreError> {
    let provider: String = row.get(0)?;
    let model: Option<String> = row.get(1)?;
    let metric: String = row.get(2)?;
    let value: String = row.get(3)?;
    let observed_at: String = row.get(4)?;
    let source: String = row.get(5)?;
    let confidence: String = row.get(6)?;
    Ok(Observation {
        provider: provider.into(),
        model: model.map(Into::into),
        metric: serde_json::from_str(&metric)?,
        value: serde_json::from_str(&value)?,
        observed_at: DateTime::parse_from_rfc3339(&observed_at)
            .map_err(|error| {
                tracing::error!(event = "stored_timestamp_invalid", field = "observed_at", error = %error, "stored observation timestamp rejected");
                StoreError::Timestamp(observed_at)
            })?
            .with_timezone(&Utc),
        source: serde_json::from_str(&source)?,
        confidence: serde_json::from_str(&confidence)?,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn observation() -> Observation {
        Observation {
            provider: "test".into(),
            model: Some("m".into()),
            metric: Metric::Tokens,
            value: MetricValue::Integer(42),
            observed_at: Utc::now(),
            source: ObservationSource::Fixture,
            confidence: Confidence::High,
        }
    }

    #[test]
    fn round_trip_and_health() {
        let file = NamedTempFile::new().expect("temp file");
        let store = Store::open(file.path()).expect("store opens");
        store.insert(&observation()).expect("insert succeeds");
        assert_eq!(store.latest().expect("latest query").len(), 1);
        store
            .heartbeat(Utc::now(), None)
            .expect("heartbeat succeeds");
        assert!(store.health().expect("health query").is_some());
        assert_eq!(store.integrity_check().expect("integrity check"), "ok");
    }
}
