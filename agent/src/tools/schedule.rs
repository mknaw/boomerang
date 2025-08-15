use reqwest::Client;
use serde_json::json;
use uuid::Uuid;

use crate::tools::tool_trait::{Tool, ToolError, ToolOutput};

pub struct ScheduleTool {
    restate_ingress_url: String,
    chat_key: String,
    platform_type: String,
    external_chat_id: String,
    adapter_key: String,
}

impl ScheduleTool {
    pub fn new(
        restate_ingress_url: String,
        chat_key: String,
        platform_type: String,
        external_chat_id: String,
        adapter_key: String,
    ) -> Self {
        Self {
            restate_ingress_url,
            chat_key,
            platform_type,
            external_chat_id,
            adapter_key,
        }
    }
}

impl Tool for ScheduleTool {
    fn name(&self) -> &str {
        "schedule"
    }

    fn description(&self) -> &str {
        r#"Schedule a task to be executed at a future time. Pass the COMPLETE user request including timing information - the scheduler will extract when to run and what to do.

Examples of requests to pass:
- "in 5 minutes check the weather in NYC"
- "tomorrow at 10AM EST remind me to call mom"
- "Monday at 8am look at Mark's code review comments"
- "every day at 9AM check my emails and summarize"

The scheduled task runs as an independent agent with its own tools and no access to the current conversation history. Include all relevant context in the request."#
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "request": {
                    "type": "string",
                    "description": "The complete user request INCLUDING timing information (e.g., 'Monday at 8am remind me to check emails'). Do NOT strip timing - pass the full request as the user said it."
                },
                "recurring": {
                    "type": "boolean",
                    "description": "Set to true if this is a recurring task (e.g., 'every day', 'every Monday'). Default is false for one-time tasks.",
                    "default": false
                }
            },
            "required": ["request"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> crate::tools::tool_trait::ToolFuture {
        let request = args["request"].as_str().unwrap_or("").to_string();
        let recurring = args["recurring"].as_bool().unwrap_or(false);

        if request.is_empty() {
            return Box::pin(
                async move { Err(ToolError::new("Request cannot be empty".to_string())) },
            );
        }

        let restate_ingress_url = self.restate_ingress_url.clone();
        let chat_key = self.chat_key.clone();
        let platform_type = self.platform_type.clone();
        let external_chat_id = self.external_chat_id.clone();
        let adapter_key = self.adapter_key.clone();

        Box::pin(async move {
            let key = Uuid::new_v4().to_string();
            let url = format!("{}/ScheduledSession/{}/run/send", restate_ingress_url, key);

            let body = json!({
                "request": request,
                "recurring": recurring,
                "chat_key": chat_key,
                "platform_type": platform_type,
                "external_chat_id": external_chat_id,
                "adapter_key": adapter_key,
            });

            let client = Client::new();
            let response = client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| ToolError::new(format!("Failed to reach Restate: {}", e)))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(ToolError::new(format!(
                    "Restate returned error {}: {}",
                    status, body
                )));
            }

            let task_type = if recurring { "Recurring task" } else { "Task" };
            Ok(ToolOutput::new(format!(
                "{} scheduled (key: {}). The scheduler will extract the timing and execute accordingly.",
                task_type, key
            )))
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }
}
