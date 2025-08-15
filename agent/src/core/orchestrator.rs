use std::sync::Arc;

use anyhow::Result;
use common::config::Config;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::{Agent, AgentId, AgentOutput, AgentRef, Artifact, Context, Task};
use crate::ai::{create_workhorse_provider, provider::Provider, session::Session, types::Message};

const MAX_SUBTASK_DEPTH: usize = 3;

pub struct OrchestratorAgent {
    id: AgentId,
    capabilities: Vec<String>,
    config: Arc<Config>,
    workers: Vec<AgentRef>,
}

impl OrchestratorAgent {
    pub fn new(id: impl Into<String>, config: Arc<Config>) -> Self {
        Self {
            id: id.into(),
            capabilities: vec!["planning".to_string(), "delegation".to_string()],
            config,
            workers: Vec::new(),
        }
    }

    pub fn with_workers(mut self, workers: Vec<AgentRef>) -> Self {
        self.workers = workers;
        self
    }

    pub fn add_worker(&mut self, worker: AgentRef) {
        self.workers.push(worker);
    }

    fn create_planner(&self) -> Result<Arc<dyn Provider>> {
        create_workhorse_provider(&self.config.ai)
    }

    async fn plan(&self, task: &Task) -> Result<TaskPlan> {
        let provider = self.create_planner()?;

        let planning_prompt = format!(
            r#"You are a task planning agent. Analyze the following task and break it down into subtasks if needed.

Task: {}

Respond with a JSON object in this exact format:
{{
    "should_decompose": true/false,
    "reasoning": "why you chose to decompose or not",
    "subtasks": [
        {{
            "id": "subtask-1",
            "description": "what this subtask should accomplish",
            "dependencies": []
        }}
    ]
}}

Guidelines:
- Only decompose if the task genuinely requires multiple distinct steps
- Each subtask should be independently executable
- Keep subtask count minimal (prefer 2-4 subtasks)
- If the task is simple enough for a single agent, set should_decompose to false"#,
            task.description
        );

        let mut session = Session::new(provider).with_system_prompt(
            "You are a task planning assistant. Always respond with valid JSON.",
        );

        session.add_message(Message::user(&planning_prompt));

        let response = session.complete().await?;
        let content = response.message.content.unwrap_or_default();

        let start = content.find('{').unwrap_or(0);
        let end = content.rfind('}').map(|i| i + 1).unwrap_or(content.len());
        let json_str = &content[start..end];

        let plan: TaskPlan = serde_json::from_str(json_str).unwrap_or_else(|_| {
            debug!("Failed to parse plan, treating as non-decomposable");
            TaskPlan {
                should_decompose: false,
                reasoning: "Failed to parse planning response".to_string(),
                subtasks: Vec::new(),
            }
        });

        Ok(plan)
    }

    async fn execute_subtasks(
        &self,
        parent_task: &Task,
        subtasks: Vec<SubtaskSpec>,
        context: &Context,
    ) -> Result<Vec<SubtaskResult>> {
        let child_context = context.child(&parent_task.id);
        let mut results = Vec::new();

        for spec in subtasks {
            let subtask = Task {
                id: format!("{}:{}", parent_task.id, spec.id),
                description: spec.description.clone(),
                parent_task_id: Some(parent_task.id.clone()),
                constraints: parent_task.constraints.clone(),
            };

            let worker = self.select_worker(&subtask);
            info!(
                "Delegating subtask {} to worker {}",
                subtask.id,
                worker.id()
            );

            match worker.execute(subtask.clone(), &child_context).await {
                Ok(output) => {
                    results.push(SubtaskResult {
                        id: spec.id,
                        success: true,
                        output: Some(output),
                        error: None,
                    });
                }
                Err(e) => {
                    results.push(SubtaskResult {
                        id: spec.id,
                        success: false,
                        output: None,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        Ok(results)
    }

    fn select_worker(&self, _task: &Task) -> AgentRef {
        self.workers.first().cloned().expect("No workers available")
    }

    async fn aggregate(&self, task: &Task, results: Vec<SubtaskResult>) -> Result<AgentOutput> {
        let provider = self.create_planner()?;

        let results_summary: Vec<String> = results
            .iter()
            .map(|r| {
                if r.success {
                    format!(
                        "Subtask '{}': SUCCESS\nResult: {}",
                        r.id,
                        r.output
                            .as_ref()
                            .map(|o| o.result.as_str())
                            .unwrap_or("No output")
                    )
                } else {
                    format!(
                        "Subtask '{}': FAILED\nError: {}",
                        r.id,
                        r.error.as_ref().unwrap_or(&"Unknown error".to_string())
                    )
                }
            })
            .collect();

        let aggregation_prompt = format!(
            r#"You are aggregating results from multiple subtasks to answer the original task.

Original Task: {}

Subtask Results:
{}

Provide a concise, coherent response that addresses the original task by synthesizing the subtask results."#,
            task.description,
            results_summary.join("\n\n")
        );

        let mut session = Session::new(provider)
            .with_system_prompt("You are an aggregation assistant. Synthesize results concisely.");

        session.add_message(Message::user(&aggregation_prompt));

        let response = session.complete().await?;
        let result = response.message.content.unwrap_or_default();

        let subtasks_spawned: Vec<String> = results.iter().map(|r| r.id.clone()).collect();

        let artifacts: Vec<Artifact> = results
            .into_iter()
            .filter_map(|r| {
                r.output
                    .map(|o| Artifact::text(format!("subtask-{}", r.id), o.result))
            })
            .collect();

        Ok(AgentOutput::new(result)
            .with_artifacts(artifacts)
            .with_subtasks(subtasks_spawned))
    }

    async fn execute_directly(&self, task: Task, context: &Context) -> Result<AgentOutput> {
        let worker = self.select_worker(&task);
        worker.execute(task, context).await
    }
}

impl Agent for OrchestratorAgent {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    fn execute<'a>(
        &'a self,
        task: Task,
        context: &'a Context,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentOutput>> + Send + 'a>> {
        Box::pin(async move {
            if self.workers.is_empty() {
                return Err(anyhow::anyhow!("Orchestrator has no workers configured"));
            }

            if context.depth >= MAX_SUBTASK_DEPTH {
                info!(
                    "Max subtask depth reached ({}), executing directly",
                    context.depth
                );
                return self.execute_directly(task, context).await;
            }

            info!("Planning task: {}", task.id);
            let plan = self.plan(&task).await?;

            if !plan.should_decompose || plan.subtasks.is_empty() {
                info!("Task does not require decomposition: {}", plan.reasoning);
                return self.execute_directly(task, context).await;
            }

            info!(
                "Decomposing task into {} subtasks: {}",
                plan.subtasks.len(),
                plan.reasoning
            );

            let results = self.execute_subtasks(&task, plan.subtasks, context).await?;
            self.aggregate(&task, results).await
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskPlan {
    should_decompose: bool,
    reasoning: String,
    subtasks: Vec<SubtaskSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SubtaskSpec {
    id: String,
    description: String,
    #[serde(default)]
    dependencies: Vec<String>,
}

#[derive(Debug)]
struct SubtaskResult {
    id: String,
    success: bool,
    output: Option<AgentOutput>,
    error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestrator_creation() {
        let config = Arc::new(Config::default());
        let orchestrator = OrchestratorAgent::new("test-orchestrator", config);

        assert_eq!(orchestrator.id(), "test-orchestrator");
        assert!(
            orchestrator
                .capabilities()
                .contains(&"planning".to_string())
        );
    }
}
