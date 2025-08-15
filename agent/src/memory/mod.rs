use std::{pin::Pin, sync::Arc};

use anyhow::Result;
use chrono::{DateTime, Utc};

pub mod file;

pub use file::FileMemory;

pub struct MemoryEntry {
    pub key: String,
    pub content: String,
    pub metadata: Option<Metadata>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct Metadata {
    pub tags: Vec<String>,
    pub source: Option<String>,
}

pub type MemoryFuture<T> = Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>;

pub trait Memory: Send + Sync {
    fn write(&self, key: &str, content: &str, metadata: Option<Metadata>) -> MemoryFuture<()>;

    fn read(&self, key: &str) -> MemoryFuture<Option<MemoryEntry>>;

    fn list(&self, prefix: Option<&str>) -> MemoryFuture<Vec<String>>;

    fn delete(&self, key: &str) -> MemoryFuture<()>;

    fn search(&self, query: &str, limit: usize) -> MemoryFuture<Vec<MemoryEntry>>;
}

pub type MemoryRef = Arc<dyn Memory>;
