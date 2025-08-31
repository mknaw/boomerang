use std::time::Duration;

use restate_sdk::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ScheduleArgs {
    minutes: u32,
    query: String,
}

#[restate_sdk::object]
pub trait ScheduledSession {
    async fn run(spec: Json<ScheduleArgs>) -> HandlerResult<()>;
}

pub struct ScheduledSessionImpl;

impl ScheduledSession for ScheduledSessionImpl {
    async fn run(&self, ctx: ObjectContext<'_>, args: Json<ScheduleArgs>) -> HandlerResult<()> {
        ctx.sleep(Duration::from_secs(args.0.minutes as u64 * 60))
            .await?;
        println!("Running scheduled query: {}", args.0.query);
        Ok(())
    }
}
