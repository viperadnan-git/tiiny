use std::hash::{DefaultHasher, Hash, Hasher};

use async_trait::async_trait;
use tower_sessions::{
    cookie::time::OffsetDateTime,
    session::{Id, Record},
    session_store::{self, SessionStore},
};

use crate::db::{Db, now_unix};

const EXTEND_AFTER_SECS: i64 = 86_400;

/// Remembers what the DB holds per session so `save` can skip writes that only
/// push the expiry forward by less than a day.
#[derive(Clone)]
pub struct LibsqlSessionStore {
    db: Db,
    stored: moka::sync::Cache<i128, (i64, u64)>,
}

impl std::fmt::Debug for LibsqlSessionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibsqlSessionStore").finish_non_exhaustive()
    }
}

fn backend<E: std::fmt::Display>(e: E) -> session_store::Error {
    session_store::Error::Backend(e.to_string())
}

fn digest(payload: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    payload.hash(&mut h);
    h.finish()
}

impl LibsqlSessionStore {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            stored: moka::sync::Cache::builder()
                .max_capacity(100_000)
                .time_to_live(std::time::Duration::from_secs(EXTEND_AFTER_SECS as u64))
                .build(),
        }
    }

    pub async fn purge_expired(&self) -> anyhow::Result<u64> {
        let conn = self.db.conn().await?;
        Ok(conn
            .execute("DELETE FROM sessions WHERE expires_at <= ?1", [now_unix()])
            .await?)
    }

    fn encode(record: &Record) -> session_store::Result<(Vec<u8>, i64, Option<i64>)> {
        // Sorted so identical data always encodes to identical bytes (HashMap order is random).
        let sorted: std::collections::BTreeMap<_, _> = record.data.iter().collect();
        let payload =
            serde_json::to_vec(&sorted).map_err(|e| session_store::Error::Encode(e.to_string()))?;
        let user_id = record
            .data
            .get(super::LOGIN_DATA_KEY)
            .and_then(|d| d.get("user_id"))
            .and_then(|v| v.as_i64());
        Ok((payload, record.expiry_date.unix_timestamp(), user_id))
    }
}

#[async_trait]
impl SessionStore for LibsqlSessionStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        let (payload, expires_at, user_id) = Self::encode(record)?;
        let conn = self.db.conn().await.map_err(backend)?;
        loop {
            let n = conn
                .execute(
                    "INSERT INTO sessions (id, payload, expires_at, user_id) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT (id) DO NOTHING",
                    libsql::params![record.id.to_string(), payload.clone(), expires_at, user_id],
                )
                .await
                .map_err(backend)?;
            if n == 1 {
                self.stored
                    .insert(record.id.0, (expires_at, digest(&payload)));
                return Ok(());
            }
            record.id = Id::default();
        }
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let (payload, expires_at, user_id) = Self::encode(record)?;
        let d = digest(&payload);
        if let Some((stored_exp, stored_d)) = self.stored.get(&record.id.0)
            && stored_d == d
            && expires_at - stored_exp < EXTEND_AFTER_SECS
        {
            return Ok(());
        }
        let conn = self.db.conn().await.map_err(backend)?;
        let n = conn
            .execute(
                // No upsert: a session deleted by a concurrent logout/revoke must stay deleted.
                "UPDATE sessions SET payload = ?2, expires_at = ?3, user_id = ?4 WHERE id = ?1",
                libsql::params![record.id.to_string(), payload, expires_at, user_id],
            )
            .await
            .map_err(backend)?;
        if n == 1 {
            self.stored.insert(record.id.0, (expires_at, d));
        }
        Ok(())
    }

    async fn load(&self, id: &Id) -> session_store::Result<Option<Record>> {
        let conn = self.db.conn().await.map_err(backend)?;
        let mut rows = conn
            .query(
                "SELECT payload, expires_at FROM sessions WHERE id = ?1 AND expires_at > ?2",
                libsql::params![id.to_string(), now_unix()],
            )
            .await
            .map_err(backend)?;
        let Some(row) = rows.next().await.map_err(backend)? else {
            self.stored.invalidate(&id.0);
            return Ok(None);
        };
        let payload: Vec<u8> = row.get(0).map_err(backend)?;
        let expires_at: i64 = row.get(1).map_err(backend)?;
        self.stored.insert(id.0, (expires_at, digest(&payload)));
        Ok(Some(Record {
            id: *id,
            data: serde_json::from_slice(&payload)
                .map_err(|e| session_store::Error::Decode(e.to_string()))?,
            expiry_date: OffsetDateTime::from_unix_timestamp(expires_at).map_err(backend)?,
        }))
    }

    async fn delete(&self, id: &Id) -> session_store::Result<()> {
        self.stored.invalidate(&id.0);
        let conn = self.db.conn().await.map_err(backend)?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", [id.to_string()])
            .await
            .map_err(backend)?;
        Ok(())
    }
}

/// Within the caller's unit of work, e.g. together with the write that revokes them.
pub async fn delete_user_sessions(conn: &libsql::Connection, user_id: i64) -> libsql::Result<()> {
    conn.execute("DELETE FROM sessions WHERE user_id = ?1", [user_id])
        .await?;
    Ok(())
}
