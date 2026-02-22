use std::time::{Duration, Instant};

use common::{
    PlatformOrigin, Turn,
    config::Config,
    restate::{
        ChatSessionClient, CronTaskStatus, ScheduleArgs, ScheduledSession, ScheduledSessionClient,
    },
};
use restate_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::{ai::types::Message, executor::AgentExecutor};

/// Unified state stored for every scheduled task (one-shot and recurring).
/// Keyed as `"cron_state"` in the virtual object's K/V store.
#[derive(Serialize, Deserialize, Clone)]
struct CronTaskState {
    task: String,
    is_recurring: bool,
    /// None for one-shot tasks.
    interval_seconds: Option<u32>,
    /// Restate invocation ID of the next pending `execute()` call.
    next_invocation_id: Option<String>,
    // Routing fields – needed to send results back to the user.
    chat_key: String,
    platform_type: String,
    external_chat_id: String,
    adapter_key: String,
    // Inspection / "first-class" fields.
    created_at: String,
    last_run_at: Option<String>,
    next_run_at: Option<String>,
    run_count: u32,
}

impl CronTaskState {
    fn into_status(self) -> CronTaskStatus {
        CronTaskStatus {
            task: self.task,
            is_recurring: self.is_recurring,
            created_at: self.created_at,
            last_run_at: self.last_run_at,
            next_run_at: self.next_run_at,
            run_count: self.run_count,
            interval_seconds: self.interval_seconds,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ScheduleExtraction {
    delay_seconds: u64,
    task: String,
    #[serde(default)]
    interval_seconds: Option<u64>,
    #[serde(default)]
    error: Option<String>,
}

const TASK_COMPLETE_MARKER: &str = "<<<TASK_COMPLETE>>>";
const STATE_KEY: &str = "cron_state";

pub struct ScheduledSessionImpl;

impl ScheduledSession for ScheduledSessionImpl {
    async fn run(&self, ctx: ObjectContext<'_>, args: Json<ScheduleArgs>) -> HandlerResult<()> {
        let args = args.0;

        info!(
            "ScheduledSession received request: '{}' (recurring: {})",
            args.request, args.recurring
        );

        let config = Config::global();

        // Extract timing and task from the request using LLM.
        let extraction = match extract_schedule_info(&args.request, args.recurring, &config).await {
            Ok(ext) => ext,
            Err(e) => {
                error!("Failed to extract schedule info: {}", e);
                let turn = Turn::scheduled_completion(
                    &args.request,
                    &format!("Failed to understand scheduling request: {}", e),
                    true,
                )
                .with_platform_origin(PlatformOrigin {
                    platform_type: args.platform_type,
                    external_chat_id: args.external_chat_id,
                    adapter_key: args.adapter_key,
                });

                if let Err(e) = ctx
                    .object_client::<ChatSessionClient>(&args.chat_key)
                    .message(Json(turn))
                    .call()
                    .await
                {
                    error!("Failed to send error to chat: {}", e);
                }
                return Ok(());
            }
        };

        if let Some(error) = extraction.error {
            warn!("Schedule extraction returned error: {}", error);
            let turn = Turn::scheduled_completion(&args.request, &error, true)
                .with_platform_origin(PlatformOrigin {
                    platform_type: args.platform_type,
                    external_chat_id: args.external_chat_id,
                    adapter_key: args.adapter_key,
                });

            if let Err(e) = ctx
                .object_client::<ChatSessionClient>(&args.chat_key)
                .message(Json(turn))
                .call()
                .await
            {
                error!("Failed to send error to chat: {}", e);
            }
            return Ok(());
        }

        info!(
            "Extracted schedule: delay={}s, task='{}', interval={:?}",
            extraction.delay_seconds, extraction.task, extraction.interval_seconds
        );

        let now = chrono::Utc::now();
        let next_run_at = now + chrono::Duration::seconds(extraction.delay_seconds as i64);

        if args.recurring || extraction.interval_seconds.is_some() {
            let interval = extraction.interval_seconds.unwrap_or(86400) as u32; // Default daily.

            debug!(
                "Starting recurring scheduled session: initial delay {}s, interval {}s, task: '{}'",
                extraction.delay_seconds, interval, extraction.task
            );

            let state = CronTaskState {
                task: extraction.task,
                is_recurring: true,
                interval_seconds: Some(interval),
                next_invocation_id: None,
                chat_key: args.chat_key,
                platform_type: args.platform_type,
                external_chat_id: args.external_chat_id,
                adapter_key: args.adapter_key,
                created_at: now.to_rfc3339(),
                last_run_at: None,
                next_run_at: Some(next_run_at.to_rfc3339()),
                run_count: 0,
            };

            let state_json = serde_json::to_string(&state).map_err(|e| {
                HandlerError::from(anyhow::anyhow!("Failed to serialize state: {}", e))
            })?;
            ctx.set(STATE_KEY, state_json);

            let handle = ctx
                .object_client::<ScheduledSessionClient>(ctx.key())
                .execute()
                .send_after(Duration::from_secs(extraction.delay_seconds));

            let inv_id = handle.invocation_id().await?;

            // Write back the invocation ID.
            let state_json: Option<String> = ctx.get(STATE_KEY).await?;
            let mut state: CronTaskState =
                state_json
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .ok_or_else(|| HandlerError::from(anyhow::anyhow!("State not found")))?;
            state.next_invocation_id = Some(inv_id.clone());
            let state_json = serde_json::to_string(&state).map_err(|e| {
                HandlerError::from(anyhow::anyhow!("Failed to serialize state: {}", e))
            })?;
            ctx.set(STATE_KEY, state_json);

            debug!(
                "Recurring session initialised, first execution scheduled with invocation id: {}",
                inv_id
            );
        } else {
            debug!(
                "Starting one-shot scheduled session: sleeping {}s before executing: '{}'",
                extraction.delay_seconds, extraction.task
            );

            // Persist state now so it is queryable during the sleep window.
            let state = CronTaskState {
                task: extraction.task.clone(),
                is_recurring: false,
                interval_seconds: None,
                next_invocation_id: None,
                chat_key: args.chat_key.clone(),
                platform_type: args.platform_type.clone(),
                external_chat_id: args.external_chat_id.clone(),
                adapter_key: args.adapter_key.clone(),
                created_at: now.to_rfc3339(),
                last_run_at: None,
                next_run_at: Some(next_run_at.to_rfc3339()),
                run_count: 0,
            };
            let state_json = serde_json::to_string(&state).map_err(|e| {
                HandlerError::from(anyhow::anyhow!("Failed to serialize state: {}", e))
            })?;
            ctx.set(STATE_KEY, state_json);

            ctx.sleep(Duration::from_secs(extraction.delay_seconds))
                .await?;

            debug!("Awoke from sleep, now executing task: {}", extraction.task);

            match execute_task(&extraction.task, &config, &args.chat_key).await {
                Ok(response) => {
                    debug!("Task completed successfully");

                    // Update state to reflect the completed run.
                    let state_json: Option<String> = ctx.get(STATE_KEY).await?;
                    if let Some(s) = state_json {
                        if let Ok(mut s) = serde_json::from_str::<CronTaskState>(&s) {
                            s.last_run_at = Some(chrono::Utc::now().to_rfc3339());
                            s.next_run_at = None;
                            s.run_count += 1;
                            if let Ok(json) = serde_json::to_string(&s) {
                                ctx.set(STATE_KEY, json);
                            }
                        }
                    }

                    let turn = Turn::scheduled_completion(&extraction.task, &response, true)
                        .with_platform_origin(PlatformOrigin {
                            platform_type: args.platform_type,
                            external_chat_id: args.external_chat_id,
                            adapter_key: args.adapter_key,
                        });

                    if let Err(e) = ctx
                        .object_client::<ChatSessionClient>(&args.chat_key)
                        .message(Json(turn))
                        .call()
                        .await
                    {
                        error!("Failed to send response to chat: {}", e);
                    }
                }
                Err(e) => {
                    error!("Task execution failed: {}", e);
                    return Err(HandlerError::from(anyhow::anyhow!(
                        "Task execution failed: {}",
                        e
                    )));
                }
            }
        }

        Ok(())
    }

    async fn execute(&self, ctx: ObjectContext<'_>) -> HandlerResult<()> {
        let state_json: Option<String> = ctx.get(STATE_KEY).await?;

        let state: CronTaskState = match state_json.and_then(|s| serde_json::from_str(&s).ok()) {
            Some(s) => s,
            None => {
                debug!("No cron state found, session was likely cancelled");
                return Ok(());
            }
        };

        let start = Instant::now();

        let augmented_task = format!(
            "{}\n\nIf the task's purpose has been completely fulfilled, include {} at the end of your response.",
            state.task, TASK_COMPLETE_MARKER
        );

        let config = Config::global();
        let llm_result = execute_task(&augmented_task, &config, &state.chat_key).await;

        let elapsed = start.elapsed().as_secs();
        let next_delay = state
            .interval_seconds
            .unwrap_or(86400)
            .saturating_sub(elapsed.try_into().unwrap_or(u32::MAX));

        let now = chrono::Utc::now();

        let (clean_response, is_complete) = match llm_result {
            Ok(response) => {
                debug!("Recurring task completed successfully");

                let is_complete = response.contains(TASK_COMPLETE_MARKER);
                let clean_response = response
                    .replace(TASK_COMPLETE_MARKER, "")
                    .trim()
                    .to_string();

                if is_complete {
                    debug!("Task completion marker found, stopping recurrence");

                    // Update state one last time before clearing.
                    let mut new_state = state.clone();
                    new_state.last_run_at = Some(now.to_rfc3339());
                    new_state.next_run_at = None;
                    new_state.next_invocation_id = None;
                    new_state.run_count += 1;
                    if let Ok(json) = serde_json::to_string(&new_state) {
                        ctx.set(STATE_KEY, json);
                    }

                    ctx.clear_all();
                } else {
                    let handle = ctx
                        .object_client::<ScheduledSessionClient>(ctx.key())
                        .execute()
                        .send_after(Duration::from_secs(next_delay as u64));

                    let inv_id = handle.invocation_id().await?;
                    let next_run_at = now + chrono::Duration::seconds(next_delay as i64);

                    let mut new_state = state.clone();
                    new_state.last_run_at = Some(now.to_rfc3339());
                    new_state.next_run_at = Some(next_run_at.to_rfc3339());
                    new_state.next_invocation_id = Some(inv_id.clone());
                    new_state.run_count += 1;
                    let state_json = serde_json::to_string(&new_state).map_err(|e| {
                        HandlerError::from(anyhow::anyhow!("Failed to serialize state: {}", e))
                    })?;
                    ctx.set(STATE_KEY, state_json);

                    debug!(
                        "Scheduled next execution in {}s (invocation id: {})",
                        next_delay, inv_id
                    );
                }

                (clean_response, is_complete)
            }
            Err(e) => {
                error!("Recurring task failed: {}", e);

                let handle = ctx
                    .object_client::<ScheduledSessionClient>(ctx.key())
                    .execute()
                    .send_after(Duration::from_secs(next_delay as u64));

                let inv_id = handle.invocation_id().await?;
                let next_run_at = now + chrono::Duration::seconds(next_delay as i64);

                let mut new_state = state.clone();
                new_state.last_run_at = Some(now.to_rfc3339());
                new_state.next_run_at = Some(next_run_at.to_rfc3339());
                new_state.next_invocation_id = Some(inv_id.clone());
                new_state.run_count += 1;
                let state_json = serde_json::to_string(&new_state).map_err(|e| {
                    HandlerError::from(anyhow::anyhow!("Failed to serialize state: {}", e))
                })?;
                ctx.set(STATE_KEY, state_json);

                debug!(
                    "Task failed but next execution scheduled in {}s (invocation id: {})",
                    next_delay, inv_id
                );

                (format!("Task failed: {}", e), false)
            }
        };

        let turn = Turn::scheduled_completion(&state.task, &clean_response, is_complete)
            .with_platform_origin(PlatformOrigin {
                platform_type: state.platform_type,
                external_chat_id: state.external_chat_id,
                adapter_key: state.adapter_key,
            });

        if let Err(e) = ctx
            .object_client::<ChatSessionClient>(&state.chat_key)
            .message(Json(turn))
            .call()
            .await
        {
            error!("Failed to send response to chat: {}", e);
        }

        Ok(())
    }

    async fn cancel(&self, ctx: ObjectContext<'_>) -> HandlerResult<()> {
        let state_json: Option<String> = ctx.get(STATE_KEY).await?;

        if let Some(state_json) = state_json
            && let Ok(state) = serde_json::from_str::<CronTaskState>(&state_json)
            && let Some(inv_id) = state.next_invocation_id
        {
            debug!("Cancelling pending invocation: {}", inv_id);
            ctx.invocation_handle(inv_id).cancel().await?;
        }

        ctx.clear_all();
        debug!("Scheduled session cancelled and state cleared");

        Ok(())
    }

    async fn status(
        &self,
        ctx: SharedObjectContext<'_>,
    ) -> HandlerResult<Json<Option<CronTaskStatus>>> {
        let state_json: Option<String> = ctx.get(STATE_KEY).await?;
        let status = state_json
            .and_then(|s| serde_json::from_str::<CronTaskState>(&s).ok())
            .map(|s| s.into_status());
        Ok(Json(status))
    }
}

fn extraction_system_prompt() -> &'static str {
    r#"You are a schedule parsing assistant. Your job is to extract timing information from user requests.

Given a scheduling request, extract:
1. delay_seconds: How many seconds from NOW until the task should first execute
2. task: The actual task to perform, with all timing/scheduling language removed
3. interval_seconds: (optional) For recurring tasks, how many seconds between executions

Return your response as valid JSON with this structure:
{
    "delay_seconds": <number>,
    "task": "<string with timing stripped>",
    "interval_seconds": <number or null>,
    "error": <string or null if no error>
}

Examples:

Input: "in 5 minutes check the weather"
Output: {"delay_seconds": 300, "task": "check the weather", "interval_seconds": null, "error": null}

Input: "tomorrow at 10AM remind me to call mom" (assuming current time is 2PM on Monday)
Output: {"delay_seconds": 72000, "task": "remind me to call mom", "interval_seconds": null, "error": null}

Input: "every hour check my email"
Output: {"delay_seconds": 3600, "task": "check my email", "interval_seconds": 3600, "error": null}

Input: "every day at 9AM summarize the news"
Output: {"delay_seconds": <seconds until next 9AM>, "task": "summarize the news", "interval_seconds": 86400, "error": null}

Input: "Monday at 8am look at Mark's code review" (assuming today is Friday)
Output: {"delay_seconds": <seconds until Monday 8AM>, "task": "look at Mark's code review", "interval_seconds": null, "error": null}

If the timing is unclear or impossible to determine, set error to a helpful message.
Always return valid JSON. Do not include any text outside the JSON object."#
}

async fn extract_schedule_info(
    request: &str,
    recurring: bool,
    config: &Config,
) -> anyhow::Result<ScheduleExtraction> {
    let executor = AgentExecutor::new_without_tools(config.clone().into());

    let prompt = format!(
        "Current time: {}\nRecurring task: {}\n\nRequest: {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z"),
        recurring,
        request
    );

    let messages = vec![Message::user(&prompt)];
    let response = executor
        .complete(&messages, extraction_system_prompt())
        .await?;

    let content = response
        .content
        .ok_or_else(|| anyhow::anyhow!("No response from extraction LLM"))?;

    debug!("Schedule extraction response: {}", content);

    // Handle case where LLM might wrap it in markdown code blocks.
    let json_str = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let extraction: ScheduleExtraction = serde_json::from_str(json_str).map_err(|e| {
        anyhow::anyhow!("Failed to parse extraction JSON: {} - raw: {}", e, content)
    })?;

    Ok(extraction)
}

async fn execute_task(task: &str, config: &Config, chat_key: &str) -> anyhow::Result<String> {
    let executor = AgentExecutor::new(config.clone().into(), chat_key.to_string());
    let messages = vec![Message::user(task)];

    let response = executor
        .complete(&messages, AgentExecutor::scheduled_system_prompt())
        .await?;

    Ok(response
        .content
        .unwrap_or_else(|| "No response content".to_string()))
}
