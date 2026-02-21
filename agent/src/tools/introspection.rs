use std::sync::Arc;

use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use serde::{Deserialize, Serialize};
use tokio_postgres::NoTls;

use crate::tools::tool_trait::{Tool, ToolError, ToolFuture, ToolOutput};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub target: String,
    pub status: String,
    pub scheduled_at: Option<String>,
    pub retry_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatus {
    pub id: String,
    pub target: String,
    pub service_name: Option<String>,
    pub service_key: Option<String>,
    pub handler: String,
    pub status: String,
    pub retry_count: i64,
    pub last_failure: Option<String>,
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemOverview {
    pub total_active: i64,
    pub pending: i64,
    pub running: i64,
    pub suspended: i64,
    pub backing_off: i64,
    pub scheduled: i64,
    pub failed_recently: i64,
}

pub struct IntrospectionClient {
    pool: Pool,
}

impl IntrospectionClient {
    pub fn new(connection_string: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut cfg = Config::new();
        cfg.url = Some(connection_string.to_string());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;
        Ok(Self { pool })
    }

    pub async fn list_scheduled_tasks(&self) -> Result<Vec<ScheduledTask>, ToolError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ToolError::new(format!("Failed to get DB connection: {}", e)))?;

        let rows = client
            .query(
                "SELECT id, target, status, scheduled_at, retry_count 
                 FROM sys_invocation 
                 WHERE status = 'scheduled'
                 ORDER BY scheduled_at ASC",
                &[],
            )
            .await
            .map_err(|e| ToolError::new(format!("Query failed: {}", e)))?;

        let tasks: Vec<ScheduledTask> = rows
            .iter()
            .map(|row| ScheduledTask {
                id: row.get(0),
                target: row.get(1),
                status: row.get(2),
                scheduled_at: row.get(3),
                retry_count: row.get(4),
            })
            .collect();

        Ok(tasks)
    }

    pub async fn list_active_tasks(&self) -> Result<Vec<TaskStatus>, ToolError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ToolError::new(format!("Failed to get DB connection: {}", e)))?;

        let rows = client
            .query(
                "SELECT id, target, target_service_name, target_service_key, 
                        target_handler_name, status, retry_count, last_failure,
                        created_at, modified_at
                 FROM sys_invocation 
                 WHERE status IN ('running', 'suspended', 'pending', 'backing-off', 'ready')
                 ORDER BY modified_at DESC
                 LIMIT 50",
                &[],
            )
            .await
            .map_err(|e| ToolError::new(format!("Query failed: {}", e)))?;

        let tasks: Vec<TaskStatus> = rows
            .iter()
            .map(|row| TaskStatus {
                id: row.get(0),
                target: row.get(1),
                service_name: row.get(2),
                service_key: row.get(3),
                handler: row.get(4),
                status: row.get(5),
                retry_count: row.get(6),
                last_failure: row.get(7),
                created_at: row.get(8),
                modified_at: row.get(9),
            })
            .collect();

        Ok(tasks)
    }

    pub async fn get_task_details(&self, task_id: &str) -> Result<Option<TaskStatus>, ToolError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ToolError::new(format!("Failed to get DB connection: {}", e)))?;

        let row = client
            .query_opt(
                "SELECT id, target, target_service_name, target_service_key, 
                        target_handler_name, status, retry_count, last_failure,
                        created_at, modified_at
                 FROM sys_invocation 
                 WHERE id = $1",
                &[&task_id],
            )
            .await
            .map_err(|e| ToolError::new(format!("Query failed: {}", e)))?;

        Ok(row.map(|row| TaskStatus {
            id: row.get(0),
            target: row.get(1),
            service_name: row.get(2),
            service_key: row.get(3),
            handler: row.get(4),
            status: row.get(5),
            retry_count: row.get(6),
            last_failure: row.get(7),
            created_at: row.get(8),
            modified_at: row.get(9),
        }))
    }

    pub async fn get_system_overview(&self) -> Result<SystemOverview, ToolError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ToolError::new(format!("Failed to get DB connection: {}", e)))?;

        let row = client
            .query_one(
                "SELECT 
                    COUNT(*) FILTER (WHERE status IN ('running', 'suspended', 'pending', 'backing-off', 'ready')) as total_active,
                    COUNT(*) FILTER (WHERE status = 'pending') as pending,
                    COUNT(*) FILTER (WHERE status = 'running') as running,
                    COUNT(*) FILTER (WHERE status = 'suspended') as suspended,
                    COUNT(*) FILTER (WHERE status = 'backing-off') as backing_off,
                    COUNT(*) FILTER (WHERE status = 'scheduled') as scheduled,
                    COUNT(*) FILTER (WHERE retry_count > 1) as failed_recently
                 FROM sys_invocation",
                &[],
            )
            .await
            .map_err(|e| ToolError::new(format!("Query failed: {}", e)))?;

        Ok(SystemOverview {
            total_active: row.get(0),
            pending: row.get(1),
            running: row.get(2),
            suspended: row.get(3),
            backing_off: row.get(4),
            scheduled: row.get(5),
            failed_recently: row.get(6),
        })
    }

    pub async fn list_pending_inbox(&self) -> Result<Vec<serde_json::Value>, ToolError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ToolError::new(format!("Failed to get DB connection: {}", e)))?;

        let rows = client
            .query(
                "SELECT service_name, service_key, id, sequence_number
                 FROM sys_inbox
                 ORDER BY sequence_number ASC
                 LIMIT 20",
                &[],
            )
            .await
            .map_err(|e| ToolError::new(format!("Query failed: {}", e)))?;

        let items: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "service": row.get::<_, String>(0),
                    "key": row.get::<_, String>(1),
                    "invocation_id": row.get::<_, String>(2),
                    "sequence": row.get::<_, i64>(3),
                })
            })
            .collect();

        Ok(items)
    }
}

pub struct ListScheduledTasksTool {
    client: Arc<IntrospectionClient>,
}

impl ListScheduledTasksTool {
    pub fn new(client: Arc<IntrospectionClient>) -> Self {
        Self { client }
    }
}

impl Tool for ListScheduledTasksTool {
    fn name(&self) -> &str {
        "list_scheduled_tasks"
    }

    fn description(&self) -> &str {
        "List all tasks scheduled for future execution. Shows when they are scheduled and what they will do."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn execute(&self, _args: serde_json::Value) -> ToolFuture {
        let client = self.client.clone();
        Box::pin(async move {
            match client.list_scheduled_tasks().await {
                Ok(tasks) => {
                    if tasks.is_empty() {
                        Ok(ToolOutput::new("No scheduled tasks found."))
                    } else {
                        let formatted: Vec<String> = tasks
                            .iter()
                            .map(|t| {
                                format!(
                                    "- {} | {} | scheduled: {:?}",
                                    t.id, t.target, t.scheduled_at
                                )
                            })
                            .collect();
                        Ok(ToolOutput::new(format!(
                            "Found {} scheduled task(s):\n{}",
                            tasks.len(),
                            formatted.join("\n")
                        )))
                    }
                }
                Err(e) => Err(e),
            }
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

pub struct ListActiveTasksTool {
    client: Arc<IntrospectionClient>,
}

impl ListActiveTasksTool {
    pub fn new(client: Arc<IntrospectionClient>) -> Self {
        Self { client }
    }
}

impl Tool for ListActiveTasksTool {
    fn name(&self) -> &str {
        "list_active_tasks"
    }

    fn description(&self) -> &str {
        "List currently active tasks (running, pending, suspended, or retrying). Shows task status and retry information."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn execute(&self, _args: serde_json::Value) -> ToolFuture {
        let client = self.client.clone();
        Box::pin(async move {
            match client.list_active_tasks().await {
                Ok(tasks) => {
                    if tasks.is_empty() {
                        Ok(ToolOutput::new("No active tasks found."))
                    } else {
                        let formatted: Vec<String> = tasks
                            .iter()
                            .map(|t| {
                                let failure_info = t
                                    .last_failure
                                    .as_ref()
                                    .map(|f| format!(" | last error: {}", f))
                                    .unwrap_or_default();
                                format!(
                                    "- {} | {} | status: {} | retries: {}{}",
                                    t.id, t.target, t.status, t.retry_count, failure_info
                                )
                            })
                            .collect();
                        Ok(ToolOutput::new(format!(
                            "Found {} active task(s):\n{}",
                            tasks.len(),
                            formatted.join("\n")
                        )))
                    }
                }
                Err(e) => Err(e),
            }
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

pub struct GetTaskDetailsTool {
    client: Arc<IntrospectionClient>,
}

impl GetTaskDetailsTool {
    pub fn new(client: Arc<IntrospectionClient>) -> Self {
        Self { client }
    }
}

impl Tool for GetTaskDetailsTool {
    fn name(&self) -> &str {
        "get_task_details"
    }

    fn description(&self) -> &str {
        "Get detailed information about a specific task by its ID."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The unique ID of the task to look up"
                }
            },
            "required": ["task_id"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolFuture {
        let client = self.client.clone();
        let task_id = args["task_id"].as_str().unwrap_or("").to_string();
        Box::pin(async move {
            if task_id.is_empty() {
                return Err(ToolError::new("task_id is required".to_string()));
            }
            match client.get_task_details(&task_id).await {
                Ok(Some(task)) => {
                    let info = format!(
                        "Task ID: {}\nTarget: {}\nStatus: {}\nService: {:?}\nKey: {:?}\nHandler: {}\nRetries: {}\nCreated: {:?}\nModified: {:?}\nLast Error: {:?}",
                        task.id,
                        task.target,
                        task.status,
                        task.service_name,
                        task.service_key,
                        task.handler,
                        task.retry_count,
                        task.created_at,
                        task.modified_at,
                        task.last_failure
                    );
                    Ok(ToolOutput::new(info))
                }
                Ok(None) => Ok(ToolOutput::new(format!("Task {} not found", task_id))),
                Err(e) => Err(e),
            }
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

pub struct GetSystemOverviewTool {
    client: Arc<IntrospectionClient>,
}

impl GetSystemOverviewTool {
    pub fn new(client: Arc<IntrospectionClient>) -> Self {
        Self { client }
    }
}

impl Tool for GetSystemOverviewTool {
    fn name(&self) -> &str {
        "get_system_overview"
    }

    fn description(&self) -> &str {
        "Get a summary of the current system state including active tasks, scheduled work, and recent failures."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn execute(&self, _args: serde_json::Value) -> ToolFuture {
        let client = self.client.clone();
        Box::pin(async move {
            match client.get_system_overview().await {
                Ok(overview) => {
                    let summary = format!(
                        "System Overview:\n\
                        - Total active tasks: {}\n\
                        - Pending: {}\n\
                        - Running: {}\n\
                        - Suspended: {}\n\
                        - Backing off (retrying): {}\n\
                        - Scheduled for future: {}\n\
                        - Failed recently: {}",
                        overview.total_active,
                        overview.pending,
                        overview.running,
                        overview.suspended,
                        overview.backing_off,
                        overview.scheduled,
                        overview.failed_recently
                    );
                    Ok(ToolOutput::new(summary))
                }
                Err(e) => Err(e),
            }
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

pub struct ListPendingQueueTool {
    client: Arc<IntrospectionClient>,
}

impl ListPendingQueueTool {
    pub fn new(client: Arc<IntrospectionClient>) -> Self {
        Self { client }
    }
}

impl Tool for ListPendingQueueTool {
    fn name(&self) -> &str {
        "list_pending_queue"
    }

    fn description(&self) -> &str {
        "List items waiting in the processing queue (inbox). These are tasks waiting to be processed."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn execute(&self, _args: serde_json::Value) -> ToolFuture {
        let client = self.client.clone();
        Box::pin(async move {
            match client.list_pending_inbox().await {
                Ok(items) => {
                    if items.is_empty() {
                        Ok(ToolOutput::new("No items in the pending queue."))
                    } else {
                        let formatted: Vec<String> = items
                            .iter()
                            .map(|item| {
                                format!(
                                    "- {} ({}): {}",
                                    item["service"].as_str().unwrap_or("unknown"),
                                    item["key"].as_str().unwrap_or("unknown"),
                                    item["invocation_id"].as_str().unwrap_or("unknown")
                                )
                            })
                            .collect();
                        Ok(ToolOutput::new(format!(
                            "Found {} item(s) in pending queue:\n{}",
                            items.len(),
                            formatted.join("\n")
                        )))
                    }
                }
                Err(e) => Err(e),
            }
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

pub fn create_introspection_tools(
    connection_string: &str,
) -> Result<Vec<Arc<dyn Tool>>, Box<dyn std::error::Error>> {
    let client = Arc::new(IntrospectionClient::new(connection_string)?);

    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(ListScheduledTasksTool::new(client.clone())),
        Arc::new(ListActiveTasksTool::new(client.clone())),
        Arc::new(GetTaskDetailsTool::new(client.clone())),
        Arc::new(GetSystemOverviewTool::new(client.clone())),
        Arc::new(ListPendingQueueTool::new(client.clone())),
    ];

    Ok(tools)
}
