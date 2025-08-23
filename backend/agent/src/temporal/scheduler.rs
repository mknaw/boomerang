use std::sync::Arc;
use std::time::Duration;
use std::collections::HashMap;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{debug, info, error};
use uuid::Uuid;
use tokio::time;

use crate::ai::provider::Provider;
use crate::tools::web_search::WebSearchTool;
use crate::Session;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    pub name: String,
    pub user_message: String,
    pub system_prompt: Option<String>,
    pub tools_enabled: bool,
    pub tavily_api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronSchedule {
    pub cron_expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelayedSchedule {
    pub delay: Duration,
}

#[derive(Debug, Clone)]
pub enum ScheduleType {
    Cron(CronSchedule),
    Delayed(DelayedSchedule),
    Immediate,
}

#[derive(Debug, Clone)]
pub struct ScheduleHandle {
    pub id: String,
    pub name: String,
    pub schedule_type: String,
}

pub struct AgentScheduler {
    scheduler: JobScheduler,
    provider: Arc<dyn Provider>,
    active_jobs: Arc<tokio::sync::Mutex<HashMap<String, uuid::Uuid>>>,
}

impl AgentScheduler {
    pub async fn new(provider: Arc<dyn Provider>) -> Result<Self> {
        let scheduler = JobScheduler::new().await?;
        
        Ok(Self {
            scheduler,
            provider,
            active_jobs: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        })
    }
    
    pub async fn start(&self) -> Result<()> {
        info!("Starting agent scheduler");
        self.scheduler.start().await?;
        Ok(())
    }
    
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down agent scheduler");
        self.scheduler.shutdown().await?;
        Ok(())
    }
    
    pub async fn schedule_agent_session(
        &self,
        config: ScheduleConfig,
        schedule_type: ScheduleType,
    ) -> Result<ScheduleHandle> {
        let schedule_id = format!("agent-session-{}", Uuid::new_v4());
        
        match schedule_type {
            ScheduleType::Immediate => {
                info!("Executing immediate agent session: {}", config.name);
                
                let provider = self.provider.clone();
                let config_clone = config.clone();
                
                tokio::spawn(async move {
                    if let Err(e) = execute_agent_session(provider, config_clone).await {
                        error!("Immediate agent session failed: {}", e);
                    }
                });
                
                Ok(ScheduleHandle {
                    id: schedule_id,
                    name: config.name,
                    schedule_type: "immediate".to_string(),
                })
            }
            
            ScheduleType::Delayed(delayed) => {
                info!("Scheduling delayed agent session: {} (delay: {:?})", config.name, delayed.delay);
                
                let provider = self.provider.clone();
                let config_clone = config.clone();
                let delay = delayed.delay;
                
                tokio::spawn(async move {
                    time::sleep(delay).await;
                    if let Err(e) = execute_agent_session(provider, config_clone).await {
                        error!("Delayed agent session failed: {}", e);
                    }
                });
                
                Ok(ScheduleHandle {
                    id: schedule_id,
                    name: config.name,
                    schedule_type: format!("delayed ({:?})", delayed.delay),
                })
            }
            
            ScheduleType::Cron(cron) => {
                info!("Scheduling cron agent session: {} (cron: {})", config.name, cron.cron_expression);
                
                let provider = self.provider.clone();
                let config_clone = config.clone();
                let schedule_name = config.name.clone();
                
                let job = Job::new_async(cron.cron_expression.as_str(), move |_uuid, _l| {
                    let provider = provider.clone();
                    let config = config_clone.clone();
                    let name = schedule_name.clone();
                    
                    Box::pin(async move {
                        info!("Executing scheduled agent session: {}", name);
                        if let Err(e) = execute_agent_session(provider, config).await {
                            error!("Cron agent session failed: {}", e);
                        }
                    })
                })?;
                
                let job_id = self.scheduler.add(job).await?;
                
                // Store the job ID for later cancellation
                let mut active_jobs = self.active_jobs.lock().await;
                active_jobs.insert(schedule_id.clone(), job_id);
                
                Ok(ScheduleHandle {
                    id: schedule_id,
                    name: config.name,
                    schedule_type: format!("cron ({})", cron.cron_expression),
                })
            }
        }
    }
    
    pub async fn cancel_scheduled_session(&self, schedule_id: &str) -> Result<()> {
        debug!("Cancelling scheduled session: {}", schedule_id);
        
        let mut active_jobs = self.active_jobs.lock().await;
        if let Some(job_id) = active_jobs.remove(schedule_id) {
            self.scheduler.remove(&job_id).await?;
            info!("Cancelled scheduled session: {}", schedule_id);
        } else {
            debug!("Schedule not found or already completed: {}", schedule_id);
        }
        
        Ok(())
    }
    
    pub async fn list_active_schedules(&self) -> Result<Vec<String>> {
        let active_jobs = self.active_jobs.lock().await;
        let schedule_ids: Vec<String> = active_jobs.keys().cloned().collect();
        debug!("Active schedules: {:?}", schedule_ids);
        Ok(schedule_ids)
    }
}

async fn execute_agent_session(
    provider: Arc<dyn Provider>,
    config: ScheduleConfig,
) -> Result<()> {
    debug!("Executing agent session: {}", config.name);
    
    let system_prompt = config.system_prompt.unwrap_or_else(|| {
        "You are a helpful assistant with access to web search.".to_string()
    });
    
    let mut session = Session::new(provider).with_system_prompt(system_prompt);
    
    if config.tools_enabled {
        if let Some(tavily_key) = config.tavily_api_key {
            let search_tool = Arc::new(WebSearchTool::new(tavily_key));
            let web_search_tool_spec = search_tool.to_tool_spec();
            session = session.with_tools(vec![web_search_tool_spec]);
        }
    }
    
    session.add_user_message(config.user_message);
    
    match session.complete().await {
        Ok(response) => {
            info!("Agent session '{}' completed successfully", config.name);
            if let Some(content) = &response.message.content {
                info!("Response: {}", content);
            }
            info!("Usage: {:?}", response.usage);
        }
        Err(e) => {
            error!("Agent session '{}' failed: {}", config.name, e);
            return Err(e.into());
        }
    }
    
    Ok(())
}