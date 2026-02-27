use anyhow::Context;
use opentelemetry::global;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use opentelemetry::trace::TracerProvider as TracerProviderTrait;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_telemetry(service_name: &str) -> anyhow::Result<SdkTracerProvider> {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .context("Failed to create OTLP span exporter")?;

    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            Resource::builder()
                .with_service_name(service_name.to_owned())
                .with_attribute(KeyValue::new("telemetry.sdk.language", "rust"))
                .build(),
        )
        .build();

    global::set_tracer_provider(tracer_provider.clone());

    let tracer = TracerProviderTrait::tracer(&tracer_provider, "rig-gemini-tracer");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt::layer().with_target(false))
        .with(otel_layer)
        .init();

    Ok(tracer_provider)
}

pub fn has_gemini_api_key() -> bool {
    std::env::var("GEMINI_API_KEY").is_ok()
}
