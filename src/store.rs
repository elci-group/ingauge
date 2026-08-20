use crate::model::*;
use rusqlite::{params, Connection};
pub struct Store {
    conn: Connection,
}
impl Store {
    pub fn open(path: &str) -> Result<Self, rusqlite::Error> {
        let c = Connection::open(path)?;
        c.execute_batch("CREATE TABLE IF NOT EXISTS observations (id INTEGER PRIMARY KEY, provider TEXT NOT NULL, model TEXT, metric TEXT NOT NULL, value TEXT NOT NULL, observed_at TEXT NOT NULL, source TEXT NOT NULL, confidence TEXT NOT NULL); CREATE INDEX IF NOT EXISTS observations_provider_time ON observations(provider,model,observed_at); CREATE INDEX IF NOT EXISTS observations_metric_time ON observations(metric,observed_at);")?;
        Ok(Self { conn: c })
    }
    pub fn insert(&self, o: &Observation) -> Result<(), rusqlite::Error> {
        self.conn.execute("INSERT INTO observations(provider,model,metric,value,observed_at,source,confidence) VALUES(?,?,?,?,?,?,?)",params![o.provider.0,o.model.as_ref().map(|x|&x.0),serde_json::to_string(&o.metric).unwrap(),serde_json::to_string(&o.value).unwrap(),o.observed_at.to_rfc3339(),serde_json::to_string(&o.source).unwrap(),serde_json::to_string(&o.confidence).unwrap()])?;
        Ok(())
    }
}
