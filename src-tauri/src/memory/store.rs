use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub timestamp: String,
    pub user: Option<String>,
    pub kind: String,
    pub content: String,
}

#[derive(Clone)]
pub struct MemoryStore {
    db: sled::Db,
}

impl MemoryStore {
    pub fn new<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }

    pub fn append(&self, kind: &str, user: Option<&str>, content: &str) -> AppResult<()> {
        let record = MemoryRecord {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            user: user.map(|x| x.to_string()),
            kind: kind.to_string(),
            content: content.to_string(),
        };
        let key = format!("{}:{}", record.timestamp, record.id);
        let value = serde_json::to_vec(&record)?;
        self.db.insert(key.as_bytes(), value)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn recent(&self, max: usize) -> AppResult<Vec<MemoryRecord>> {
        let mut out = Vec::new();
        for item in self.db.iter().rev().take(max) {
            let (_, val) = item?;
            let rec: MemoryRecord = serde_json::from_slice(&val)
                .map_err(|e| AppError::Storage(format!("memory parse error: {e}")))?;
            out.push(rec);
        }
        Ok(out)
    }

    pub fn clear(&self) -> AppResult<()> {
        self.db.clear()?;
        self.db.flush()?;
        Ok(())
    }
}
