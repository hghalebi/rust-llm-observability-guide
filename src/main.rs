use opentelemetry::trace::{TraceContextExt, Tracer};
use opentelemetry::{KeyValue, global};
use opentelemetry_otlp::{SpanExporter, WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::TracerProvider;
use tonic::metadata;

fn init_tracer() ->TracerProvider{
    let zignoze_ingestion_key = std::env::var("SIGNOZ_INGESTION_KEY").expect("SIGNOZ_INGESTION_KEY is not set");
    let zignoze_end_point = std::env::var("SIGNOZ_ENDPOINT").expect("SIGNOZ_ENDPOINT is not set");
    let app_name = "telemetry learning";
    let mut metadata = tonic::metadata::MetadataMap::new();
    metadata.insert(
        "zignoze_ingestion_key",
        metadata::MetadataValue::try_from(zignoze_ingestion_key).unwrap()
    );

    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_tls_config(tonic::transport::ClientTlsConfig::new().with_native_roots())
        .with_metadata(metadata)
        .with_endpoint(zignoze_end_point)
        .build()
        .expect("Failed to create span exporter");
    let resources = Resource::new(
        vec![
            KeyValue::new("service.name",app_name)
        ]
    );
    TracerProvider::builder()
        .with_resource(resources)
        .with_batch_exporter(exporter,opentelemetry_sdk::runtime::Tokio)
        .build()

    
}
#[tokio::main]
async fn main() {
    println!("Hello, world!");
}
