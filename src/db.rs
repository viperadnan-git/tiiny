use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering::SeqCst},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::Context;

use crate::config::Config;

// In-house runner: libsql_migration pulls a second libsql version (type mismatch).
/// `migrations/*.sql`, embedded in release builds (read from disk in debug), like the web assets.
#[derive(rust_embed::Embed)]
#[folder = "migrations"]
struct MigrationFiles;

/// `NNNN_name.sql` files by version, ascending; a bad name or a repeated version is a startup error.
fn migrations() -> anyhow::Result<Vec<(i64, String)>> {
    let mut out = vec![];
    for name in MigrationFiles::iter().filter(|n| n.ends_with(".sql")) {
        let version = name
            .split_once('_')
            .and_then(|(v, _)| v.parse::<i64>().ok())
            .with_context(|| format!("migration {name}: name must start with a number and `_`"))?;
        let file = MigrationFiles::get(&name).expect("listed by iter");
        let sql = String::from_utf8(file.data.into_owned())
            .with_context(|| format!("migration {name}: not UTF-8"))?;
        out.push((version, sql));
    }
    out.sort_by_key(|(v, _)| *v);
    if let Some(w) = out.windows(2).find(|w| w[0].0 == w[1].0) {
        anyhow::bail!("two migrations share version {}", w[0].0);
    }
    Ok(out)
}

async fn pragma(conn: &libsql::Connection, sql: &str) -> libsql::Result<()> {
    let mut rows = conn.query(sql, ()).await?;
    while rows.next().await?.is_some() {}
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Local,
    /// Embedded replica: reads local, writes to the primary; see [`Db::catch_up`].
    Replica,
    /// CLI: straight to the primary over Hrana.
    Remote,
}

const POOL_IDLE_MAX: usize = 16;
/// Under Turso's Hrana stream idle timeout (10s): a `query` leaves the connection's stream open.
const POOL_IDLE_TTL: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct Db {
    db: Arc<libsql::Database>,
    mode: Mode,
    /// Replica only: libsql's `connect()` builds a runtime and an HTTP client (new TLS handshake) each time.
    idle: Arc<Mutex<Vec<(Instant, libsql::Connection)>>>,
    /// Replica only: units of work that wrote, and how many of them the local file has.
    written: Arc<AtomicU64>,
    synced: Arc<AtomicU64>,
    pulling: Arc<tokio::sync::Mutex<()>>,
}

impl Db {
    fn new(db: libsql::Database, mode: Mode) -> Db {
        Db {
            db: Arc::new(db),
            mode,
            idle: Default::default(),
            written: Default::default(),
            synced: Default::default(),
            pulling: Default::default(),
        }
    }

    pub async fn open(config: &Config) -> anyhow::Result<Db> {
        std::fs::create_dir_all(&config.data_dir)
            .with_context(|| format!("creating data dir {}", config.data_dir.display()))?;
        let path = config.data_dir.join("tiiny.db");
        let (db, mode) = match &config.database_url {
            Some(url) => (
                libsql::Builder::new_remote_replica(
                    path,
                    url.clone(),
                    config.database_auth_token.clone().unwrap_or_default(),
                )
                .sync_interval(config.replica_sync_interval)
                // Default pulls after every remote statement (BEGIN and COMMIT included); catch_up pulls once per unit of work.
                .read_your_writes(false)
                .build()
                .await,
                Mode::Replica,
            ),
            None => (libsql::Builder::new_local(path).build().await, Mode::Local),
        };
        let db = Db::new(db.context("opening database")?, mode);
        if mode == Mode::Local {
            let conn = db.conn().await?;
            pragma(conn.reader(), "PRAGMA journal_mode = WAL").await?;
        }
        Ok(db)
    }

    /// For the CLI: in replica mode talks to the primary directly, leaving the server's replica file alone.
    pub async fn open_direct(config: &Config) -> anyhow::Result<Db> {
        let Some(url) = &config.database_url else {
            return Self::open(config).await;
        };
        let db = libsql::Builder::new_remote(
            url.clone(),
            config.database_auth_token.clone().unwrap_or_default(),
        )
        .build()
        .await
        .context("opening database")?;
        Ok(Db::new(db, Mode::Remote))
    }

    async fn setup(&self, conn: &libsql::Connection) -> libsql::Result<()> {
        match self.mode {
            Mode::Local => {
                pragma(conn, "PRAGMA busy_timeout = 5000").await?;
                // SQLite defaults to off; Turso enforces foreign keys itself.
                conn.execute("PRAGMA foreign_keys = ON", ()).await?;
            }
            // Local file: libsql runs statements it can't classify locally; background pulls write to it.
            Mode::Replica => pragma(conn, "PRAGMA busy_timeout = 5000").await?,
            // Turso rejects busy_timeout over Hrana.
            Mode::Remote => {}
        }
        Ok(())
    }

    /// A connection for one unit of work, reads and writes alike. Bind it (`let conn = db.conn().await?`)
    /// so rows drop before it: in replica mode it goes back to a pool on drop. Writes go to the primary;
    /// reads are local and see every write made through a [`Conn`] dropped before this call. Reads inside
    /// a transaction go to the primary, so decision reads belong inside the write transaction.
    pub async fn conn(&self) -> anyhow::Result<Conn> {
        if let Err(e) = self.catch_up().await {
            // Stale reads until the next pull beat failing every read while the primary is unreachable.
            tracing::warn!(error = ?e, "replica catch-up failed");
        }
        let pooled = (self.mode == Mode::Replica)
            .then(|| {
                let mut idle = self.idle.lock().unwrap();
                idle.retain(|(at, _)| at.elapsed() < POOL_IDLE_TTL);
                idle.pop().map(|(_, c)| c)
            })
            .flatten();
        let conn = match pooled {
            Some(c) => c,
            None => {
                let c = self.db.connect()?;
                self.setup(&c).await?;
                c
            }
        };
        Ok(Conn {
            conn: Some(conn),
            db: self.clone(),
            wrote: AtomicBool::new(false),
        })
    }

    /// Pulls the primary into the local file if a unit of work wrote since the last pull. Replica only.
    pub async fn catch_up(&self) -> anyhow::Result<()> {
        let needed = self.written.load(SeqCst);
        if self.mode != Mode::Replica || self.synced.load(SeqCst) >= needed {
            return Ok(());
        }
        // Waiters coalesce: a pull that started after their writes were recorded covers them.
        let _pulling = self.pulling.lock().await;
        if self.synced.load(SeqCst) >= needed {
            return Ok(());
        }
        let target = self.written.load(SeqCst);
        self.db.sync().await.context("syncing replica")?;
        self.synced.fetch_max(target, SeqCst);
        Ok(())
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        let conn = self.conn().await?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at INTEGER) STRICT",
            (),
        )
        .await?;
        if self.mode == Mode::Replica {
            // The version check below reads the local file: a fresh or old one lacks the primary's rows.
            self.db.sync().await.context("syncing replica")?;
        }
        for (version, sql) in migrations()? {
            let mut applied = conn
                .query(
                    "SELECT 1 FROM schema_migrations WHERE version = ?1",
                    [version],
                )
                .await?;
            if applied.next().await?.is_some() {
                continue;
            }
            drop(applied);
            let tx = conn.transaction().await?;
            tx.execute_batch(&sql).await?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                [version, now_unix()],
            )
            .await?;
            tx.commit().await?;
        }
        Ok(())
    }
}

/// Pulls the request's writes into the local file before its response leaves, so the next reader
/// (a redirect, say) doesn't pay for the pull.
pub async fn sync_writes(
    axum::extract::State(db): axum::extract::State<Db>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let res = next.run(req).await;
    if let Err(e) = db.catch_up().await {
        tracing::warn!(error = ?e, "replica catch-up failed");
    }
    res
}

/// [`Db::conn`]'s guard. No `Deref`: every write goes through a method that records it, so the
/// replica catches up before the next read (libsql's counters miss `query` writes).
pub struct Conn {
    conn: Option<libsql::Connection>,
    db: Db,
    wrote: AtomicBool,
}

impl Conn {
    fn raw(&self) -> &libsql::Connection {
        self.conn.as_ref().expect("taken only on drop")
    }

    /// For helpers that write.
    pub fn writer(&self) -> &libsql::Connection {
        self.wrote.store(true, SeqCst);
        self.raw()
    }

    /// For helpers that only read.
    pub fn reader(&self) -> &libsql::Connection {
        self.raw()
    }

    /// Anything but a plain `SELECT` counts as a write.
    pub async fn query(
        &self,
        sql: &str,
        params: impl libsql::params::IntoParams,
    ) -> libsql::Result<libsql::Rows> {
        let read = sql
            .trim_start()
            .get(..6)
            .is_some_and(|w| w.eq_ignore_ascii_case("select"));
        let conn = if read { self.raw() } else { self.writer() };
        conn.query(sql, params).await
    }

    pub async fn execute(
        &self,
        sql: &str,
        params: impl libsql::params::IntoParams,
    ) -> libsql::Result<u64> {
        self.writer().execute(sql, params).await
    }

    pub async fn transaction(&self) -> libsql::Result<libsql::Transaction> {
        self.writer().transaction().await
    }

    pub async fn transaction_with_behavior(
        &self,
        behavior: libsql::TransactionBehavior,
    ) -> libsql::Result<libsql::Transaction> {
        self.writer().transaction_with_behavior(behavior).await
    }

    pub fn last_insert_rowid(&self) -> i64 {
        self.raw().last_insert_rowid()
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        let (Some(conn), Mode::Replica) = (self.conn.take(), self.db.mode) else {
            return;
        };
        if *self.wrote.get_mut() {
            self.db.written.fetch_add(1, SeqCst);
        }
        // Mid-transaction (dropped Transaction, cancelled request): dropping closes the remote stream.
        // Known gap: a request cancelled inside BEGIN can leave libsql's parser state off; it resets on its next error.
        if conn.is_autocommit() {
            let mut idle = self.db.idle.lock().unwrap();
            if idle.len() < POOL_IDLE_MAX {
                idle.push((Instant::now(), conn));
            }
        }
    }
}

/// Hourly housekeeping besides sessions: spent email tokens, link quota rows before yesterday.
pub async fn purge_stale(db: &Db) -> anyhow::Result<u64> {
    let conn = db.conn().await?;
    let tokens = conn
        .execute(
            "DELETE FROM email_tokens WHERE used_at IS NOT NULL OR expires_at <= ?1",
            [now_unix()],
        )
        .await?;
    let cutoff = today_utc_day() - 1;
    let counts = conn
        .execute("DELETE FROM link_quota WHERE epoch_day < ?1", [cutoff])
        .await?;
    Ok(tokens + counts)
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub fn today_utc_day() -> i64 {
    now_unix() / 86400
}

pub async fn prune_stats(db: &Db, retention_days: u32) -> anyhow::Result<u64> {
    let cutoff = today_utc_day() - i64::from(retention_days);
    let conn = db.conn().await?;
    Ok(conn
        .execute("DELETE FROM link_stats WHERE epoch_day < ?1", [cutoff])
        .await?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn migrations_are_found_in_order() {
        let m = super::migrations().unwrap();
        assert_eq!(m.first().map(|(v, _)| *v), Some(1));
        assert!(m.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(m[0].1.contains("CREATE TABLE users"));
    }
}
