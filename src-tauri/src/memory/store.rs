use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    #[serde(default = "default_memory_schema_version")]
    pub schema_version: u8,
    pub id: String,
    pub timestamp: String,
    pub user: Option<String>,
    pub kind: String,
    pub content: String,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

fn default_memory_schema_version() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedMemoryRecord {
    pub id: String,
    pub label: String,
    pub content: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct MemoryStore {
    db: sled::Db,
    pinned: sled::Tree,
    log_path: PathBuf,
}

impl MemoryStore {
    pub fn new<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        let root = path.as_ref().to_path_buf();
        let db = sled::open(&root)?;
        let pinned = db
            .open_tree("pinned_memory")
            .map_err(AppError::from)?;
        let log_path = root.join("memory_log.jsonl");
        Ok(Self { db, pinned, log_path })
    }

    pub fn append(&self, kind: &str, user: Option<&str>, content: &str) -> AppResult<()> {
        self.append_structured(kind, user, content, None, 0, vec![], None)
    }

    pub fn append_structured(
        &self,
        kind: &str,
        user: Option<&str>,
        content: &str,
        subject: Option<String>,
        priority: u8,
        tags: Vec<String>,
        metadata: Option<Value>,
    ) -> AppResult<()> {
        let record = MemoryRecord {
            schema_version: default_memory_schema_version(),
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            user: user.map(|x| x.to_string()),
            kind: kind.to_string(),
            content: content.to_string(),
            subject,
            priority,
            tags,
            metadata,
        };
        let key = format!("{}:{}", record.timestamp, record.id);
        let value = serde_json::to_vec(&record)?;
        self.db.insert(key.as_bytes(), value)?;
        self.db.flush()?;
        let rendered = serde_json::to_string(&record)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .map_err(|e| AppError::Storage(format!("memory log open failed {}: {e}", self.log_path.display())))?;
        writeln!(file, "{rendered}")
            .map_err(|e| AppError::Storage(format!("memory log write failed {}: {e}", self.log_path.display())))?;
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
        self.pinned.clear().map_err(AppError::from)?;
        self.pinned.flush().map_err(AppError::from)?;
        std::fs::write(&self.log_path, "")
            .map_err(|e| AppError::Storage(format!("memory log clear failed {}: {e}", self.log_path.display())))?;
        Ok(())
    }

    pub fn log_path(&self) -> String {
        self.log_path.to_string_lossy().to_string()
    }

    pub fn tail(&self, max: usize) -> AppResult<Vec<MemoryRecord>> {
        let raw = std::fs::read_to_string(&self.log_path).unwrap_or_default();
        let mut out = Vec::new();
        for line in raw.lines().rev().take(max) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let rec: MemoryRecord = serde_json::from_str(trimmed)
                .map_err(|e| AppError::Storage(format!("memory log parse error: {e}")))?;
            out.push(rec);
        }
        Ok(out)
    }

    pub fn list_pinned(&self) -> AppResult<Vec<PinnedMemoryRecord>> {
        let mut out = Vec::new();
        for item in self.pinned.iter() {
            let (_, val) = item.map_err(AppError::from)?;
            let rec: PinnedMemoryRecord = serde_json::from_slice(&val)
                .map_err(|e| AppError::Storage(format!("pinned memory parse error: {e}")))?;
            out.push(rec);
        }
        out.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
        Ok(out)
    }

    pub fn upsert_pinned(&self, label: &str, content: &str) -> AppResult<PinnedMemoryRecord> {
        let key = label.trim().to_lowercase();
        let existing = self
            .pinned
            .get(key.as_bytes())
            .map_err(AppError::from)?
            .and_then(|raw| serde_json::from_slice::<PinnedMemoryRecord>(&raw).ok());
        let record = PinnedMemoryRecord {
            id: existing
                .as_ref()
                .map(|v| v.id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            label: label.trim().to_string(),
            content: content.trim().to_string(),
            updated_at: Utc::now().to_rfc3339(),
        };
        let value = serde_json::to_vec(&record)?;
        self.pinned
            .insert(key.as_bytes(), value)
            .map_err(AppError::from)?;
        self.pinned.flush().map_err(AppError::from)?;
        Ok(record)
    }

    pub fn delete_pinned(&self, label: &str) -> AppResult<bool> {
        let key = label.trim().to_lowercase();
        let removed = self
            .pinned
            .remove(key.as_bytes())
            .map_err(AppError::from)?
            .is_some();
        self.pinned.flush().map_err(AppError::from)?;
        Ok(removed)
    }
}
