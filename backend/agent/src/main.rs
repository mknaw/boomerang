use agent::config::Config;
use agent::scheduler::ScheduledSession;
use agent::scheduler::ScheduledSessionImpl;
use restate_sdk::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    let config = Config::load().expect("Failed to load configuration");
    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    
    HttpServer::new(
        Endpoint::builder()
            .bind(ScheduledSessionImpl.serve())
            .build(),
    )
    .listen_and_serve(bind_addr.parse().unwrap())
    .await;
}
