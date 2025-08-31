use agent::{
    config::Config,
    scheduler::{ScheduledSession, ScheduledSessionImpl},
};
use restate_sdk::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    Config::init().expect("Failed to initialize configuration");
    let config = Config::global();
    let bind_addr = format!("{}:{}", config.server.host, config.server.port);

    HttpServer::new(
        Endpoint::builder()
            .bind(ScheduledSessionImpl.serve())
            .build(),
    )
    .listen_and_serve(bind_addr.parse().unwrap())
    .await;
}
