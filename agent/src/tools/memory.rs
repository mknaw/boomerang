use std::sync::Arc;

use serde_json::json;

use crate::{
    memory::{MemoryRef, Metadata},
    tools::tool_trait::{Tool, ToolError, ToolFuture, ToolOutput},
};

pub struct WriteMemoryTool {
    memory: MemoryRef,
}

impl WriteMemoryTool {
    pub fn new(memory: MemoryRef) -> Self {
        Self { memory }
    }
}

impl Tool for WriteMemoryTool {
    fn name(&self) -> &str {
        "memory_write"
    }

    fn description(&self) -> &str {
        "Write content to persistent memory storage. Use this to save notes, facts, or any information you want to remember across conversations."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "Unique identifier for this memory entry (e.g., 'user_preferences', 'project_notes')"
                },
                "content": {
                    "type": "string",
                    "description": "The content to store"
                },
                "tags": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Optional tags for categorization and search"
                }
            },
            "required": ["key", "content"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolFuture {
        let memory = self.memory.clone();

        Box::pin(async move {
            let key = args["key"]
                .as_str()
                .ok_or_else(|| ToolError::new("Missing required parameter: key"))?;

            let content = args["content"]
                .as_str()
                .ok_or_else(|| ToolError::new("Missing required parameter: content"))?;

            let tags: Vec<String> = args["tags"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            let metadata = if tags.is_empty() {
                None
            } else {
                Some(Metadata { tags, source: None })
            };

            match memory.write(key, content, metadata).await {
                Ok(()) => Ok(ToolOutput::new(format!(
                    "Successfully saved memory entry: {}",
                    key
                ))),
                Err(e) => Err(ToolError::new(format!("Failed to write memory: {}", e))),
            }
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }
}

pub struct ReadMemoryTool {
    memory: MemoryRef,
}

impl ReadMemoryTool {
    pub fn new(memory: MemoryRef) -> Self {
        Self { memory }
    }
}

impl Tool for ReadMemoryTool {
    fn name(&self) -> &str {
        "memory_read"
    }

    fn description(&self) -> &str {
        "Read content from persistent memory storage by key. Use this to retrieve previously saved information."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "The key of the memory entry to read"
                }
            },
            "required": ["key"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolFuture {
        let memory = self.memory.clone();

        Box::pin(async move {
            let key = args["key"]
                .as_str()
                .ok_or_else(|| ToolError::new("Missing required parameter: key"))?;

            match memory.read(key).await {
                Ok(Some(entry)) => {
                    let mut output = vec![format!("Key: {}", entry.key)];

                    if let Some(ref metadata) = entry.metadata
                        && !metadata.tags.is_empty()
                    {
                        output.push(format!("Tags: {}", metadata.tags.join(", ")));
                    }

                    output.push(format!(
                        "Created: {}",
                        entry.created_at.format("%Y-%m-%d %H:%M:%S UTC")
                    ));
                    output.push(format!(
                        "Updated: {}",
                        entry.updated_at.format("%Y-%m-%d %H:%M:%S UTC")
                    ));
                    output.push(String::new());
                    output.push(entry.content);

                    Ok(ToolOutput::new(output.join("\n")))
                }
                Ok(None) => Ok(ToolOutput::new(format!(
                    "No memory entry found for key: {}",
                    key
                ))),
                Err(e) => Err(ToolError::new(format!("Failed to read memory: {}", e))),
            }
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

pub struct ListMemoryTool {
    memory: MemoryRef,
}

impl ListMemoryTool {
    pub fn new(memory: MemoryRef) -> Self {
        Self { memory }
    }
}

impl Tool for ListMemoryTool {
    fn name(&self) -> &str {
        "memory_list"
    }

    fn description(&self) -> &str {
        "List all memory entries, optionally filtered by prefix. Use this to see what information is stored."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "prefix": {
                    "type": "string",
                    "description": "Optional prefix to filter memory keys"
                }
            }
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolFuture {
        let memory = self.memory.clone();

        Box::pin(async move {
            let prefix = args["prefix"].as_str();

            match memory.list(prefix).await {
                Ok(keys) => {
                    if keys.is_empty() {
                        Ok(ToolOutput::new("No memory entries found.".to_string()))
                    } else {
                        let output = format!(
                            "Memory entries ({} total):\n{}",
                            keys.len(),
                            keys.join("\n")
                        );
                        Ok(ToolOutput::new(output))
                    }
                }
                Err(e) => Err(ToolError::new(format!("Failed to list memory: {}", e))),
            }
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

pub struct SearchMemoryTool {
    memory: MemoryRef,
}

impl SearchMemoryTool {
    pub fn new(memory: MemoryRef) -> Self {
        Self { memory }
    }
}

impl Tool for SearchMemoryTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn description(&self) -> &str {
        "Search memory entries using ripgrep. Supports both literal text and regex patterns. \
         Use literal search for exact phrases like 'user authentication'. \
         Use regex patterns like 'async\\s+fn\\s+\\w+' or 'error_\\d+' for flexible matching. \
         Search is case-insensitive by default. The query is automatically interpreted as regex if valid, otherwise as literal text."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query to match against keys, tags, and content"
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum number of results to return",
                    "default": 10
                }
            },
            "required": ["query"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolFuture {
        let memory = self.memory.clone();

        Box::pin(async move {
            let query = args["query"]
                .as_str()
                .ok_or_else(|| ToolError::new("Missing required parameter: query"))?;

            let limit = args["limit"].as_u64().unwrap_or(10) as usize;

            match memory.search(query, limit).await {
                Ok(entries) => {
                    if entries.is_empty() {
                        Ok(ToolOutput::new(format!(
                            "No memory entries found matching query: {}",
                            query
                        )))
                    } else {
                        let mut output = vec![format!("Search results for '{}':", query)];

                        for (i, entry) in entries.iter().enumerate() {
                            output.push(format!("\n[{}] Key: {}", i + 1, entry.key));

                            if let Some(ref metadata) = entry.metadata
                                && !metadata.tags.is_empty()
                            {
                                output.push(format!("    Tags: {}", metadata.tags.join(", ")));
                            }

                            let preview = if entry.content.len() > 200 {
                                format!("{}...", &entry.content[..200])
                            } else {
                                entry.content.clone()
                            };
                            output.push(format!("    Preview: {}", preview));
                        }

                        Ok(ToolOutput::new(output.join("\n")))
                    }
                }
                Err(e) => Err(ToolError::new(format!("Failed to search memory: {}", e))),
            }
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

pub struct DeleteMemoryTool {
    memory: MemoryRef,
}

impl DeleteMemoryTool {
    pub fn new(memory: MemoryRef) -> Self {
        Self { memory }
    }
}

impl Tool for DeleteMemoryTool {
    fn name(&self) -> &str {
        "memory_delete"
    }

    fn description(&self) -> &str {
        "Delete a memory entry by key. Use this to remove outdated or incorrect information."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "The key of the memory entry to delete"
                }
            },
            "required": ["key"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolFuture {
        let memory = self.memory.clone();

        Box::pin(async move {
            let key = args["key"]
                .as_str()
                .ok_or_else(|| ToolError::new("Missing required parameter: key"))?;

            match memory.delete(key).await {
                Ok(()) => Ok(ToolOutput::new(format!(
                    "Successfully deleted memory entry: {}",
                    key
                ))),
                Err(e) => Err(ToolError::new(format!("Failed to delete memory: {}", e))),
            }
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }
}

pub fn create_memory_tools(memory: MemoryRef) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(WriteMemoryTool::new(memory.clone())),
        Arc::new(ReadMemoryTool::new(memory.clone())),
        Arc::new(ListMemoryTool::new(memory.clone())),
        Arc::new(SearchMemoryTool::new(memory.clone())),
        Arc::new(DeleteMemoryTool::new(memory.clone())),
    ]
}
