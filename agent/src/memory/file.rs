use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, sinks::UTF8};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{debug, error};

use super::{Memory, MemoryEntry, MemoryFuture, Metadata};

#[derive(Debug, Serialize, Deserialize)]
struct IndexEntry {
    key: String,
    path: PathBuf,
    tags: Vec<String>,
    source: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MemoryIndex {
    entries: Vec<IndexEntry>,
}

pub struct FileMemory {
    base_path: PathBuf,
}

impl FileMemory {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        let base_path = base_path.into();
        Self { base_path }
    }

    pub async fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.base_path).await.with_context(|| {
            format!(
                "Failed to create memory directory: {}",
                self.base_path.display()
            )
        })?;

        let index_path = self.index_path();
        if !index_path.exists() {
            let index = MemoryIndex::default();
            let index_json = serde_json::to_string_pretty(&index)?;
            fs::write(&index_path, index_json).await.with_context(|| {
                format!("Failed to create index file: {}", index_path.display())
            })?;
        }

        debug!("FileMemory initialized at: {}", self.base_path.display());
        Ok(())
    }

    fn index_path(&self) -> PathBuf {
        self.base_path.join(".index.json")
    }

    fn entry_path(&self, key: &str) -> PathBuf {
        self.base_path.join(format!("{}.md", key))
    }

    fn sanitize_key(key: &str) -> String {
        key.chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => c,
            })
            .collect()
    }

    async fn load_index_static(base_path: &Path) -> Result<MemoryIndex> {
        let index_path = base_path.join(".index.json");
        if !index_path.exists() {
            return Ok(MemoryIndex::default());
        }

        let content = fs::read_to_string(&index_path)
            .await
            .with_context(|| format!("Failed to read index file: {}", index_path.display()))?;

        let index: MemoryIndex = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse index file: {}", index_path.display()))?;

        Ok(index)
    }

    async fn save_index_static(base_path: &Path, index: &MemoryIndex) -> Result<()> {
        let index_path = base_path.join(".index.json");
        let content = serde_json::to_string_pretty(index)?;
        fs::write(&index_path, content)
            .await
            .with_context(|| format!("Failed to write index file: {}", index_path.display()))?;
        Ok(())
    }

    async fn read_static(base_path: &Path, key: &str) -> Result<Option<MemoryEntry>> {
        let sanitized_key = Self::sanitize_key(key);
        let index = Self::load_index_static(base_path).await?;

        let entry = match index.entries.iter().find(|e| e.key == sanitized_key) {
            Some(e) => e,
            None => return Ok(None),
        };

        let content = match fs::read_to_string(&entry.path).await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to read memory file {}: {}", entry.path.display(), e);
                return Ok(None);
            }
        };

        Ok(Some(MemoryEntry {
            key: entry.key.clone(),
            content,
            metadata: Some(Metadata {
                tags: entry.tags.clone(),
                source: entry.source.clone(),
            }),
            created_at: entry.created_at,
            updated_at: entry.updated_at,
        }))
    }
}

impl Memory for FileMemory {
    fn write(&self, key: &str, content: &str, metadata: Option<Metadata>) -> MemoryFuture<()> {
        let sanitized_key = Self::sanitize_key(key);
        let entry_path = self.entry_path(&sanitized_key);
        let now = Utc::now();
        let base_path = self.base_path.clone();
        let content = content.to_string();

        Box::pin(async move {
            fs::create_dir_all(&base_path).await.with_context(|| {
                format!("Failed to create memory directory: {}", base_path.display())
            })?;

            fs::write(&entry_path, &content).await.with_context(|| {
                format!("Failed to write memory entry: {}", entry_path.display())
            })?;

            let mut index = FileMemory::load_index_static(&base_path).await?;
            let existing_pos = index.entries.iter().position(|e| e.key == sanitized_key);

            let entry = IndexEntry {
                key: sanitized_key.clone(),
                path: entry_path,
                tags: metadata
                    .as_ref()
                    .map(|m| m.tags.clone())
                    .unwrap_or_default(),
                source: metadata.as_ref().and_then(|m| m.source.clone()),
                created_at: existing_pos
                    .map(|pos| index.entries[pos].created_at)
                    .unwrap_or(now),
                updated_at: now,
            };

            if let Some(pos) = existing_pos {
                index.entries[pos] = entry;
            } else {
                index.entries.push(entry);
            }

            FileMemory::save_index_static(&base_path, &index).await?;
            debug!("Wrote memory entry: {}", sanitized_key);

            Ok(())
        })
    }

    fn read(&self, key: &str) -> MemoryFuture<Option<MemoryEntry>> {
        let sanitized_key = Self::sanitize_key(key);
        let base_path = self.base_path.clone();

        Box::pin(async move {
            let index = FileMemory::load_index_static(&base_path).await?;

            let entry = match index.entries.iter().find(|e| e.key == sanitized_key) {
                Some(e) => e,
                None => return Ok(None),
            };

            let content = match fs::read_to_string(&entry.path).await {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to read memory file {}: {}", entry.path.display(), e);
                    return Ok(None);
                }
            };

            Ok(Some(MemoryEntry {
                key: entry.key.clone(),
                content,
                metadata: Some(Metadata {
                    tags: entry.tags.clone(),
                    source: entry.source.clone(),
                }),
                created_at: entry.created_at,
                updated_at: entry.updated_at,
            }))
        })
    }

    fn list(&self, prefix: Option<&str>) -> MemoryFuture<Vec<String>> {
        let base_path = self.base_path.clone();
        let prefix = prefix.map(|s| s.to_string());

        Box::pin(async move {
            let index = FileMemory::load_index_static(&base_path).await?;

            let keys: Vec<String> = index
                .entries
                .iter()
                .filter(|e| {
                    prefix
                        .as_ref()
                        .map(|p| e.key.starts_with(p))
                        .unwrap_or(true)
                        && e.path.exists()
                })
                .map(|e| e.key.clone())
                .collect();

            Ok(keys)
        })
    }

    fn delete(&self, key: &str) -> MemoryFuture<()> {
        let sanitized_key = Self::sanitize_key(key);
        let entry_path = self.entry_path(&sanitized_key);
        let base_path = self.base_path.clone();

        Box::pin(async move {
            if entry_path.exists() {
                fs::remove_file(&entry_path).await.with_context(|| {
                    format!("Failed to delete memory entry: {}", entry_path.display())
                })?;
            }

            let mut index = FileMemory::load_index_static(&base_path).await?;
            index.entries.retain(|e| e.key != sanitized_key);
            FileMemory::save_index_static(&base_path, &index).await?;

            debug!("Deleted memory entry: {}", sanitized_key);
            Ok(())
        })
    }

    fn search(&self, query: &str, limit: usize) -> MemoryFuture<Vec<MemoryEntry>> {
        let base_path = self.base_path.clone();
        let query = query.to_string();

        Box::pin(async move {
            let index = FileMemory::load_index_static(&base_path).await?;
            let mut results = Vec::new();

            // Try regex with case-insensitive flag, fall back to escaped literal
            let pattern = format!("(?i){}", query);
            let matcher = RegexMatcher::new_line_matcher(&pattern).unwrap_or_else(|_| {
                RegexMatcher::new_line_matcher(&regex::escape(&query)).unwrap()
            });

            for entry in &index.entries {
                if results.len() >= limit {
                    break;
                }

                let file_path = base_path.join(&entry.path);
                if !file_path.exists() {
                    continue;
                }

                // Search the file content using ripgrep
                let mut found_match = false;
                let sink = UTF8(|_lnum, _line| {
                    found_match = true;
                    Ok(false) // Stop after first match
                });

                if Searcher::new()
                    .search_path(&matcher, &file_path, sink)
                    .is_ok()
                    && found_match
                {
                    if let Ok(Some(memory_entry)) =
                        FileMemory::read_static(&base_path, &entry.key).await
                    {
                        results.push(memory_entry);
                    }
                }
            }

            Ok(results)
        })
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_file_memory_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let memory = FileMemory::new(temp_dir.path());
        memory.init().await.unwrap();

        // Test write
        memory
            .write("test-key", "Test content", None)
            .await
            .unwrap();

        // Test read
        let entry = memory.read("test-key").await.unwrap();
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().content, "Test content");

        // Test list
        let keys = memory.list(None).await.unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], "test-key");

        // Test delete
        memory.delete("test-key").await.unwrap();
        let entry = memory.read("test-key").await.unwrap();
        assert!(entry.is_none());
    }

    #[tokio::test]
    async fn test_file_memory_search() {
        let temp_dir = TempDir::new().unwrap();
        let memory = FileMemory::new(temp_dir.path());
        memory.init().await.unwrap();

        memory
            .write(
                "notes",
                "This is a note about Rust programming",
                Some(Metadata {
                    tags: vec!["rust".to_string(), "programming".to_string()],
                    source: None,
                }),
            )
            .await
            .unwrap();

        memory
            .write(
                "todo",
                "Buy milk and eggs",
                Some(Metadata {
                    tags: vec!["shopping".to_string()],
                    source: None,
                }),
            )
            .await
            .unwrap();

        // Test literal search (case-insensitive)
        let results = memory.search("rust", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "notes");

        // Test case-insensitive literal search
        let results = memory.search("RUST", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "notes");

        // Test regex pattern (word boundary)
        let results = memory.search(r"\bnote\b", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "notes");

        // Test regex pattern (multiple words)
        let results = memory.search(r"milk.*eggs", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "todo");

        // Test limit
        let results = memory.search(r"\w+", 1).await.unwrap();
        assert_eq!(results.len(), 1);
    }
}
