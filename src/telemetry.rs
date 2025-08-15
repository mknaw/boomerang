use std::time::Duration;

use opentelemetry::global;
use opentelemetry_otlp::{MetricExporter, WithExportConfig};
use opentelemetry_sdk::{metrics::PeriodicReader, runtime::Tokio};
use tracing::info;

pub struct TelemetryGuard {
    _meter_provider: opentelemetry_sdk::metrics::SdkMeterProvider,
}

pub fn init_telemetry(
    otlp_endpoint: &str,
    service_name: &str,
    export_interval_secs: u64,
) -> TelemetryGuard {
    // OTLP HTTP endpoints need the full path
    let metrics_endpoint = format!("{}/v1/metrics", otlp_endpoint.trim_end_matches('/'));
    let traces_endpoint = format!("{}/v1/traces", otlp_endpoint.trim_end_matches('/'));

    info!(
        "Initializing OpenTelemetry metrics to: {}",
        metrics_endpoint
    );
    info!("Initializing OpenTelemetry traces to: {}", traces_endpoint);

    let meter_provider = init_metrics(&metrics_endpoint, service_name, export_interval_secs);
    global::set_meter_provider(meter_provider.clone());

    // Create and set up tracer provider too
    init_traces(&traces_endpoint, service_name);

    info!("OpenTelemetry initialization complete");

    // Record a startup metric to verify the connection
    let meter = global::meter("boomerang");
    let startup_counter = meter
        .u64_counter("app.startup")
        .with_description("Application startup counter")
        .build();
    startup_counter.add(1, &[]);
    info!("Recorded startup metric");

    TelemetryGuard {
        _meter_provider: meter_provider,
    }
}

fn init_metrics(
    otlp_endpoint: &str,
    service_name: &str,
    export_interval_secs: u64,
) -> opentelemetry_sdk::metrics::SdkMeterProvider {
    let exporter = MetricExporter::builder()
        .with_http()
        .with_endpoint(otlp_endpoint)
        .with_timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to create OTLP metric exporter");

    let reader = PeriodicReader::builder(exporter, Tokio)
        .with_interval(Duration::from_secs(export_interval_secs))
        .build();

    opentelemetry_sdk::metrics::SdkMeterProvider::builder()
        .with_resource(opentelemetry_sdk::Resource::new(vec![
            opentelemetry::KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                service_name.to_string(),
            ),
            opentelemetry::KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_VERSION,
                env!("CARGO_PKG_VERSION"),
            ),
        ]))
        .with_reader(reader)
        .build()
}

fn init_traces(otlp_endpoint: &str, service_name: &str) {
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::TracerProvider;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(otlp_endpoint)
        .with_timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to create OTLP trace exporter");

    let provider = TracerProvider::builder()
        .with_resource(opentelemetry_sdk::Resource::new(vec![
            opentelemetry::KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                service_name.to_string(),
            ),
            opentelemetry::KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_VERSION,
                env!("CARGO_PKG_VERSION"),
            ),
        ]))
        .with_batch_exporter(exporter, Tokio)
        .build();

    global::set_tracer_provider(provider);

    info!("Tracer provider initialized");
}

pub fn create_otel_layer(
    otlp_endpoint: &str,
    service_name: &str,
) -> tracing_opentelemetry::OpenTelemetryLayer<
    tracing_subscriber::Registry,
    opentelemetry_sdk::trace::Tracer,
> {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::TracerProvider;

    let traces_endpoint = format!("{}/v1/traces", otlp_endpoint.trim_end_matches('/'));

    info!("Creating tracing OTLP layer to: {}", traces_endpoint);

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(&traces_endpoint)
        .with_timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to create OTLP trace exporter");

    let provider = TracerProvider::builder()
        .with_resource(opentelemetry_sdk::Resource::new(vec![
            opentelemetry::KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                service_name.to_string(),
            ),
            opentelemetry::KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_VERSION,
                env!("CARGO_PKG_VERSION"),
            ),
        ]))
        .with_batch_exporter(exporter, Tokio)
        .build();

    let tracer = provider.tracer("boomerang");
    global::set_tracer_provider(provider);

    tracing_opentelemetry::layer().with_tracer(tracer)
}
