use anyhow::Context;
use opentelemetry::trace::{Span, Tracer};
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::{env, time::Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let marker = env::var("OTEL_SMOKE_MARKER").unwrap_or_else(|_| "learn-smoke".to_owned());
    let endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:4317".to_string());
    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .context("Failed to create span exporter")?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();
    global::set_tracer_provider(provider.clone());

    let tracer = global::tracer("otel-smoke-direct");
    let mut span = tracer.start("otel_smoke_probe");
    span.set_attributes(vec![KeyValue::new("marker", marker.to_owned())]);
    tokio::time::sleep(Duration::from_millis(200)).await;
    span.end();

    provider.shutdown().context("Failed to shutdown provider")?;
    println!("otel smoke probe complete marker={marker}");
    Ok(())
}
