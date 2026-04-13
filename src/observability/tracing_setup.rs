use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize the tracing subscriber for structured logging.
///
/// If `OTEL_EXPORTER_OTLP_ENDPOINT` is set, OpenTelemetry trace export is configured
/// via the OTLP exporter. Otherwise, traces go only to the tracing subscriber as
/// structured JSON — zero network overhead.
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("clearmemory=info,warn"));

    let fmt_layer = fmt::layer().with_target(true);

    if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() {
        match init_otel_tracer() {
            Ok(tracer) => {
                let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
                tracing_subscriber::registry()
                    .with(filter)
                    .with(fmt_layer)
                    .with(otel_layer)
                    .init();
                tracing::info!("OpenTelemetry tracing enabled");
                return;
            }
            Err(e) => {
                eprintln!("Failed to initialize OpenTelemetry: {e}. Falling back to stdout.");
            }
        }
    }

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();
}

fn init_otel_tracer() -> Result<opentelemetry_sdk::trace::Tracer, opentelemetry::trace::TraceError>
{
    use opentelemetry::trace::TracerProvider as _;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()?;

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .build();

    let tracer = provider.tracer("clearmemory");
    opentelemetry::global::set_tracer_provider(provider);
    Ok(tracer)
}

/// Shutdown OpenTelemetry on process exit (flush pending spans).
pub fn shutdown_tracing() {
    opentelemetry::global::shutdown_tracer_provider();
}
