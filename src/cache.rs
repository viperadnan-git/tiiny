use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::SeqCst},
    },
    time::{Duration, Instant},
};

use axum::http::HeaderValue;
use moka::{Expiry, sync::Cache};

use crate::{config::Config, db::Db};

const MISSING_TTL: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub enum CachedSlug {
    Found {
        link_id: i64,
        /// The `Location` value, built once per cache fill.
        url: HeaderValue,
        permanent: bool,
    },
    Missing,
    Retired,
    Blocked,
}

struct SlugExpiry(Duration);

impl Expiry<String, CachedSlug> for SlugExpiry {
    fn expire_after_create(&self, _: &String, v: &CachedSlug, _: Instant) -> Option<Duration> {
        Some(match v {
            CachedSlug::Found { .. } => self.0,
            CachedSlug::Missing | CachedSlug::Retired | CachedSlug::Blocked => MISSING_TTL,
        })
    }
}

/// `generation` is bumped by every evict so a lookup that raced one doesn't re-cache stale data.
#[derive(Clone)]
pub struct SlugCache {
    cache: Cache<String, CachedSlug>,
    generation: Arc<AtomicU64>,
}

impl SlugCache {
    pub fn new(config: &Config) -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(config.cache_max_entries)
                .expire_after(SlugExpiry(config.cache_ttl))
                .build(),
            generation: Default::default(),
        }
    }

    pub async fn resolve(&self, db: &Db, slug: &str) -> anyhow::Result<CachedSlug> {
        if let Some(v) = self.cache.get(slug) {
            return Ok(v);
        }
        let generation = self.generation.load(SeqCst);
        let v = lookup(db, slug).await?;
        self.store(generation, slug, v.clone());
        Ok(v)
    }

    // Checked after the insert: an evict either bumped the generation before the check, or its invalidate runs after the insert.
    fn store(&self, generation: u64, slug: &str, v: CachedSlug) {
        self.cache.insert(slug.to_owned(), v);
        if self.generation.load(SeqCst) != generation {
            self.cache.invalidate(slug);
        }
    }

    pub fn evict(&self, slug: &str) {
        self.generation.fetch_add(1, SeqCst);
        self.cache.invalidate(slug);
    }

    pub fn evict_all(&self) {
        self.generation.fetch_add(1, SeqCst);
        self.cache.invalidate_all();
    }
}

async fn lookup(db: &Db, slug: &str) -> anyhow::Result<CachedSlug> {
    let conn = db.conn().await?;
    let mut rows = conn
        .query(
            // Pinned: without ANALYZE stats the planner prefers uq_links_slug and reads the table row too.
            "SELECT l.id, l.url, l.permanent_at IS NOT NULL, l.blocked_at IS NOT NULL \
             FROM links l INDEXED BY idx_links_slug_redirect \
             LEFT JOIN users u ON u.id = l.user_id \
             WHERE l.slug = ?1 AND (u.id IS NULL OR u.disabled_at IS NULL)",
            [slug],
        )
        .await?;
    if let Some(r) = rows.next().await? {
        if r.get::<i64>(3)? != 0 {
            return Ok(CachedSlug::Blocked);
        }
        return Ok(CachedSlug::Found {
            link_id: r.get(0)?,
            url: HeaderValue::try_from(r.get::<String>(1)?)?,
            permanent: r.get::<i64>(2)? != 0,
        });
    }
    Ok(
        if crate::links::model::slug_retired(conn.reader(), slug).await? {
            CachedSlug::Retired
        } else {
            CachedSlug::Missing
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_racing_an_evict_is_not_cached() {
        let config =
            Config::from_vars([("TIINY_BASE_URL".into(), "http://t.test".into())]).unwrap();
        let c = SlugCache::new(&config);
        let g = c.generation.load(SeqCst);
        c.evict("abc");
        c.store(g, "abc", CachedSlug::Missing);
        assert!(c.cache.get("abc").is_none());
        let g = c.generation.load(SeqCst);
        c.store(g, "abc", CachedSlug::Missing);
        assert!(c.cache.get("abc").is_some());
    }
}
