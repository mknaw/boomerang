use std::time::Duration;

use restate_sdk::{context::RequestTarget, prelude::*};

/// This example shows how to implement a periodic task, by invoking itself in a loop.
///
/// The `start()` handler schedules the first call to `run()`, and then each `run()` will re-schedule itself.
///
/// To "break" the loop, we use a flag we persist in state, which is removed when `stop()` is invoked.
/// Its presence determines whether the task is active or not.
///
/// To start it:
///
/// ```shell
/// $ curl -v http://localhost:8080/PeriodicTask/my-periodic-task/start
/// ```
#[restate_sdk::object]
trait PeriodicTask {
    /// Schedules the periodic task to start
    async fn start() -> HandlerResult<()>;
    /// Stops the periodic task
    async fn stop() -> HandlerResult<()>;
    /// Business logic of the periodic task
    async fn run() -> HandlerResult<()>;
}

struct PeriodicTaskImpl;

const ACTIVE: &str = "active";

impl PeriodicTask for PeriodicTaskImpl {
    async fn start(&self, context: ObjectContext<'_>) -> HandlerResult<()> {
        if context
            .get::<bool>(ACTIVE)
            .await?
            .is_some_and(|enabled| enabled)
        {
            // If it's already activated, just do nothing
            return Ok(());
        }

        // Schedule the periodic task
        PeriodicTaskImpl::schedule_next(&context);

        // Mark the periodic task as active
        context.set(ACTIVE, true);

        Ok(())
    }

    async fn stop(&self, context: ObjectContext<'_>) -> HandlerResult<()> {
        // Remove the active flag
        context.clear(ACTIVE);

        Ok(())
    }

    async fn run(&self, context: ObjectContext<'_>) -> HandlerResult<()> {
        if context.get::<bool>(ACTIVE).await?.is_none() {
            // Task is inactive, do nothing
            return Ok(());
        }

        // --- Periodic task business logic!
        println!("Triggered the periodic task!");

        // Schedule the periodic task
        PeriodicTaskImpl::schedule_next(&context);

        Ok(())
    }
}

impl PeriodicTaskImpl {
    fn schedule_next(context: &ObjectContext<'_>) {
        context.send(
            RequestTarget::object("PeriodicTask", context.key(), "run"),
            (),
            Some(Duration::from_secs(10)),
        );
        // To schedule, create a client to the callee handler (in this case, we're calling ourselves)
        // context
        // .object_client::<PeriodicTask>(context.key())
        // .run("foo", PeriodicTaskImpl {});
        // And send with a delay
        // .send_after(Duration::from_secs(10));
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    HyperServer::new(
        Endpoint::builder()
            .with_service(PeriodicTaskImpl.serve())
            .build(),
    )
    .listen_and_serve("0.0.0.0:9080".parse().unwrap())
    .await;
}
