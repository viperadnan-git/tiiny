use std::time::Duration;

use clap::Parser;
use tiiny::{
    AppState, Cli, CliOutcome, auth::session_store::LibsqlSessionStore, build_router,
    config::Config, db::prune_stats, run_cli,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // Real environment variables win over `.env`.
    if let Err(e) = dotenvy::dotenv()
        && !e.not_found()
    {
        eprintln!(".env error: {e}");
        std::process::exit(1);
    }
    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config error: {e}");
            std::process::exit(1);
        }
    };
    let serve = matches!(cli.command, None | Some(tiiny::Command::Serve));
    let _log_guard = match tiiny::logging::init(&config, serve) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("logging error: {e}");
            std::process::exit(1);
        }
    };
    let config = match run_cli(cli, config).await {
        Ok(CliOutcome::Serve(c)) => *c,
        Ok(CliOutcome::Done(out)) => {
            println!("{out}");
            return Ok(());
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let (host, port) = (config.host.clone(), config.port);
    if config.base_url.is_none() {
        tracing::warn!(
            "TIINY_BASE_URL unset: cookies are not Secure, email and OAuth are off, no sitemap; set it in production"
        );
    }
    let dummy_hash = tokio::task::spawn_blocking(|| {
        std::sync::LazyLock::force(&tiiny::auth::password::DUMMY_HASH);
    });
    let db = tiiny::db::Db::open(&config).await?;
    db.migrate().await?;
    let promoted =
        tiiny::auth::backend::promote_listed(db.conn().await?.writer(), &config, None).await?;
    if promoted > 0 {
        tracing::info!(promoted, "promoted TIINY_ADMIN_EMAILS accounts to admin");
    }
    let state = AppState::new(config, db)?;
    dummy_hash.await?;
    let (stop_flush, flush_task) = spawn_stats_tasks(&state);
    spawn_session_purge(&state);
    tiiny::reports::notify::spawn(state.clone());
    let app = build_router(state.clone());
    let listener = tokio::net::TcpListener::bind((host.as_str(), port)).await?;
    let addr = listener.local_addr()?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        %addr,
        database = if state.config.database_url.is_some() { "turso replica" } else { "local file" },
        email = state.mailer.is_enabled(),
        google = state.config.google.is_some(),
        github = state.config.github.is_some(),
        signup = state.config.signup_enabled,
        guest_links = state.config.guest_links_enabled,
        log_file = state.config.log_file,
        "listening"
    );
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    let _ = stop_flush.send(true);
    let _ = flush_task.await;
    if let Err(e) = state.stats.flush(&state.db).await {
        tracing::error!(error = ?e, "final stats flush failed");
    }
    Ok(())
}

fn spawn_stats_tasks(
    state: &AppState,
) -> (
    tokio::sync::watch::Sender<bool>,
    tokio::task::JoinHandle<()>,
) {
    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    let s = state.clone();
    let flush_task = tokio::spawn(async move {
        let mut tick = tokio::time::interval(s.config.stats_flush_interval);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            // Stop only between ticks: dropping a flush mid-write would lose its swapped-out batch.
            tokio::select! {
                _ = tick.tick() => {}
                _ = stopped.changed() => return,
            }
            if let Err(e) = s.stats.flush(&s.db).await {
                tracing::error!(error = ?e, "stats flush failed");
            }
        }
    });
    let Some(days) = state.config.stats_retention_days else {
        return (stop, flush_task);
    };
    let db = state.db.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let mut tick = tokio::time::interval(Duration::from_secs(86_400));
        loop {
            tick.tick().await;
            match prune_stats(&db, days).await {
                Ok(n) => tracing::info!(rows = n, "pruned stats"),
                Err(e) => tracing::error!(error = ?e, "stats retention failed"),
            }
        }
    });
    (stop, flush_task)
}

fn spawn_session_purge(state: &AppState) {
    let store = LibsqlSessionStore::new(state.db.clone());
    let db = state.db.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            match store.purge_expired().await {
                Ok(n) => tracing::debug!(rows = n, "purged expired sessions"),
                Err(e) => tracing::error!(error = ?e, "session purge failed"),
            }
            match tiiny::db::purge_stale(&db).await {
                Ok(n) => tracing::debug!(rows = n, "purged stale tokens and counters"),
                Err(e) => tracing::error!(error = ?e, "purge failed"),
            }
        }
    });
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}
