use crate::rest::RestResponse;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub collected_at: i64,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub duration_ms: i64,
    pub request_body: Option<String>,
    pub response_body: String,
}

#[derive(Debug, Clone)]
pub struct NewHistoryEntry {
    pub method: String,
    pub url: String,
    pub status: u16,
    pub duration_ms: i64,
    pub request_body: Option<String>,
    pub response_body: String,
}

impl NewHistoryEntry {
    pub fn from_exchange(
        method: &str,
        url: &str,
        request_body: Option<String>,
        response: &RestResponse,
    ) -> Self {
        Self {
            method: method.to_string(),
            url: url.to_string(),
            status: response.status,
            duration_ms: response.duration_ms as i64,
            request_body,
            response_body: response.body.clone(),
        }
    }
}

pub struct HistoryStore {
    conn: rusqlite::Connection,
}

impl HistoryStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                collected_at INTEGER NOT NULL,
                method TEXT NOT NULL,
                url TEXT NOT NULL,
                status INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL,
                request_body TEXT,
                response_body TEXT NOT NULL
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn append(&self, entry: NewHistoryEntry) -> Result<i64> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.conn.execute(
            "INSERT INTO history (collected_at, method, url, status, duration_ms, request_body, response_body)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                now,
                entry.method,
                entry.url,
                entry.status,
                entry.duration_ms,
                entry.request_body,
                entry.response_body,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_recent(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, collected_at, method, url, status, duration_ms, request_body, response_body
             FROM history ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                collected_at: row.get(1)?,
                method: row.get(2)?,
                url: row.get(3)?,
                status: row.get::<_, i64>(4)? as u16,
                duration_ms: row.get(5)?,
                request_body: row.get(6)?,
                response_body: row.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get(&self, id: i64) -> Result<Option<HistoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, collected_at, method, url, status, duration_ms, request_body, response_body
             FROM history WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([id], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                collected_at: row.get(1)?,
                method: row.get(2)?,
                url: row.get(3)?,
                status: row.get::<_, i64>(4)? as u16,
                duration_ms: row.get(5)?,
                request_body: row.get(6)?,
                response_body: row.get(7)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn append_and_list_recent() {
        let dir = tempdir().unwrap();
        let store = HistoryStore::open(&dir.path().join("history.db")).unwrap();
        let id1 = store
            .append(NewHistoryEntry {
                method: "GET".into(),
                url: "http://a".into(),
                status: 200,
                duration_ms: 10,
                request_body: None,
                response_body: "a".into(),
            })
            .unwrap();
        let id2 = store
            .append(NewHistoryEntry {
                method: "POST".into(),
                url: "http://b".into(),
                status: 201,
                duration_ms: 20,
                request_body: Some("{}".into()),
                response_body: "b".into(),
            })
            .unwrap();
        assert!(id2 > id1);
        let recent = store.list_recent(10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].url, "http://b");
        assert_eq!(store.get(id1).unwrap().unwrap().url, "http://a");
    }
}
