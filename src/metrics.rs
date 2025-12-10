// src/metrics.rs
//
// Basic metrics + tracing helpers for Shrimpl.
//
// This file is self-contained and uses `tracing` as the primary API.
// If OpenTelemetry is configured, traces can be exported; otherwise,
// everything still works as logs.
//
// Environment-driven initialization:
//
//   SHRIMPL_OTEL=1                   # enable otel pipeline
//   SHRIMPL_SERVICE_NAME=shrimpl-app # optional service name
//
// You can also ignore OpenTelemetry entirely and just rely on tracing.

use std::env;

use once_cell::sync::OnceCell;
use tracing::{info, instrument};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

static INIT_ONCE: OnceCell<()> = OnceCell::new();

pub fn init_from_env() {
    INIT_ONCE.get_or_init(|| {
        let enable_otel = env::var("SHRIMPL_OTEL")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        if enable_otel {
            // If you add opentelemetry crates, you can wire them here
            // (left as a hook to keep core crate light).
            let fmt_layer = fmt::layer().with_target(false);
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
        } else {
            let fmt_layer = fmt::layer().with_target(false);
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
        }

        info!(
            "[shrimpl-metrics] tracing initialized (otel={})",
            enable_otel
        );
    });
}

/// Increment a named counter by `value`.
pub fn metrics_incr(name: &str, value: f64) {
    info!(counter = name, value, "[metric] incr");
}

/// Record a gauge (point-in-time) value.
pub fn metrics_gauge(name: &str, value: f64) {
    info!(gauge = name, value, "[metric] gauge");
}

/// Trace a named span around a closure. Intended to be used by
/// interpreter / HTTP handlers.
#[instrument(name = "shrimpl_span", skip(f))]
pub fn trace_span<T, F>(span_name: &str, f: F) -> T
where
    F: FnOnce() -> T,
{
    info!(span = span_name, "[trace] span begin");
    let out = f();
    info!(span = span_name, "[trace] span end");
    out
}
