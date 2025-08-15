use chrono::{DateTime, Utc};
use dropshot::{
    ApiDescription, ConfigDropshot, ConfigLogging, HttpError, HttpResponseCreated, HttpResponseOk,
    RequestContext, TypedBody, endpoint,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct Schedule {
    id: Uuid,
    name: String,
    description: String,
    schedule: String,
    #[serde(rename = "isActive")]
    is_active: bool,
    #[serde(rename = "createdAt")]
    created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateScheduleRequest {
    name: String,
    description: String,
    schedule: String,
    #[serde(rename = "isActive")]
    is_active: Option<bool>,
}

type ApiState = ();

#[endpoint {
    method = GET,
    path = "/schedules",
}]
async fn get_schedules(
    _rqctx: RequestContext<ApiState>,
) -> Result<HttpResponseOk<Vec<Schedule>>, HttpError> {
    let dummy_schedules = vec![
        Schedule {
            id: Uuid::new_v4(),
            name: "Morning Email Check".to_string(),
            description: "Check emails every morning and notify if important".to_string(),
            schedule: "0 8 * * 1-5".to_string(),
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        Schedule {
            id: Uuid::new_v4(),
            name: "Weather Update".to_string(),
            description: "Get weather forecast for the day".to_string(),
            schedule: "0 7 * * *".to_string(),
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
    ];

    Ok(HttpResponseOk(dummy_schedules))
}

#[endpoint {
    method = POST,
    path = "/schedules",
}]
async fn create_schedule(
    _rqctx: RequestContext<ApiState>,
    body: TypedBody<CreateScheduleRequest>,
) -> Result<HttpResponseCreated<Schedule>, HttpError> {
    let request = body.into_inner();
    let now = Utc::now();

    let schedule = Schedule {
        id: Uuid::new_v4(),
        name: request.name,
        description: request.description,
        schedule: request.schedule,
        is_active: request.is_active.unwrap_or(true),
        created_at: now,
        updated_at: now,
    };

    Ok(HttpResponseCreated(schedule))
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut api = ApiDescription::new();
    api.register(get_schedules).unwrap();
    api.register(create_schedule).unwrap();

    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = 3333;

    let config = ConfigDropshot {
        bind_address: format!("{}:{}", host, port).parse().unwrap(),
        request_body_max_bytes: 1024 * 1024,
        default_handler_task_mode: dropshot::HandlerTaskMode::Detached,
        log_headers: vec![],
    };

    let log_config = ConfigLogging::StderrTerminal {
        level: dropshot::ConfigLoggingLevel::Info,
    };
    let logger = log_config.to_logger("boomerang").unwrap();

    let server = dropshot::HttpServerStarter::new(&config, api, (), &logger)
        .map_err(|error| format!("failed to create server: {}", error))?
        .start();

    println!("Server running on http://{}:{}", host, port);
    server.await
}
