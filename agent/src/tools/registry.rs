use std::{collections::HashMap, sync::Arc};

use serde_json::json;
use tracing::warn;

use super::tool_trait::ToolRef;

pub struct ToolRegistry {
    tools: HashMap<String, ToolRef>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: ToolRef) {
        let name = tool.name().to_string();
        if self.tools.contains_key(&name) {
            warn!("Tool '{}' is being overwritten in registry", name);
        }
        self.tools.insert(name, tool);
    }

    pub fn get(&self, name: &str) -> Option<ToolRef> {
        self.tools.get(name).cloned()
    }

    pub fn specs(&self) -> Vec<rmcp::model::Tool> {
        self.tools
            .iter()
            .map(|(name, tool)| {
                let schema = tool.parameters_schema();
                let mut tool_map = serde_json::Map::new();
                tool_map.insert("type".to_string(), json!("object"));
                tool_map.insert(
                    "properties".to_string(),
                    schema
                        .get("properties")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                );
                if let Some(required) = schema.get("required") {
                    tool_map.insert("required".to_string(), required.clone());
                }

                let description = tool.description().to_string();
                rmcp::model::Tool {
                    name: name.clone().into(),
                    description: Some(description.into()),
                    input_schema: Arc::new(tool_map),
                    output_schema: None,
                    annotations: None,
                }
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    pub fn tools(&self) -> Vec<ToolRef> {
        self.tools.values().cloned().collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
