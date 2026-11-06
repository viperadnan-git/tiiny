//! Text on stdout, JSON lines in daily files. No IPs or emails: events carry ids.

use std::{io::IsTerminal, time::Duration};

use axum::http::{Request, Response};
use tracing::Span;
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::Config;

/// The session crates are quieted: their per-request `call` spans and per-stale-cookie warnings add nothing.
/// axum-login is off: it logs every backend `Err` (a disabled account signing in) at error level, and real
/// failures already reach `AppError`, which logs internal ones.
pub const DEFAULT_FILTER: &str =
    "info,axum_login=off,tower_sessions=warn,tower_sessions_core=error";

/// One file per UTC day, a week kept.
const KEEP_DAYS: usize = 7;
const SLOW: Duration = Duration::from_secs(1);

/// `file` is off for CLI commands so they never rotate the server's log. Keep the guard alive: dropping it flushes.
pub fn init(config: &Config, file: bool) -> anyhow::Result<Option<WorkerGuard>> {
    let filter = EnvFilter::try_new(config.log.as_deref().unwrap_or(DEFAULT_FILTER))?;
    let stdout = fmt::layer()
        .with_target(cfg!(debug_assertions))
        .with_ansi(std::io::stdout().is_terminal());
    let (file_layer, guard) = if file && config.log_file {
        // Absolute, so the `latest.log` symlink resolves; created first, or the appender's first prune complains.
        let raw = config.data_dir.join("logs");
        let dir = std::fs::create_dir_all(&raw)
            .and_then(|()| raw.canonicalize())
            .map_err(|e| anyhow::anyhow!("log directory {}: {e}", raw.display()))?;
        // tiiny.YYYY-MM-DD.log; `latest.log` lacks the prefix, so pruning never counts it.
        let appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("tiiny")
            .filename_suffix("log")
            .max_log_files(KEEP_DAYS)
            .latest_symlink("latest.log")
            .build(&dir)
            .map_err(|e| anyhow::anyhow!("log directory {}: {e}", dir.display()))?;
        let (writer, guard) = tracing_appender::non_blocking(appender);
        let layer = fmt::layer()
            .json()
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(true)
            .with_writer(writer);
        (Some(layer.boxed()), Some(guard))
    } else {
        (None, None)
    };
    tracing_subscriber::registry()
        .with(filter)
        .with(stdout)
        .with(file_layer)
        .try_init()?;
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::capture();
        tracing::error!(panic = %info, %backtrace, "panic");
    }));
    Ok(guard)
}

/// Request span: every event in a handler carries these. The query string is left out (it may hold tokens).
pub fn request_span<B>(req: &Request<B>) -> Span {
    let id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    tracing::info_span!("request", id, method = %req.method(), path = req.uri().path())
}

/// Every response at debug; slow ones at warn. Server errors are logged with their cause by `AppError`.
pub fn on_response<B>(res: &Response<B>, latency: Duration, _: &Span) {
    let status = res.status().as_u16();
    let latency_ms = latency.as_millis() as u64;
    if latency >= SLOW {
        tracing::warn!(status, latency_ms, "slow request");
    } else {
        // Microseconds too: scripts/bench.py reads them (redirect timings are well under a millisecond).
        let latency_us = latency.as_micros() as u64;
        tracing::debug!(status, latency_ms, latency_us, "request");
    }
}
