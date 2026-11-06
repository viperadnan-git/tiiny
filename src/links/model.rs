use std::sync::Arc;

use garde::Valid;
use serde::{Deserialize, Serialize};

use crate::{
    config::Config,
    db::{Db, now_unix, today_utc_day},
    error::AppError,
    validate,
};

use super::slug::{custom_slug, random_base62};

/// Also for retired slugs: users aren't told a slug once existed.
pub const SLUG_TAKEN: &str = "slug is already taken. Try another";

#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub id: i64,
    pub user_id: Option<i64>,
    pub slug: String,
    pub url: String,
    pub click_count: i64,
    pub permanent: bool,
    pub blocked: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct UserRef {
    pub id: i64,
    pub is_admin: bool,
}

pub enum Owner {
    User(i64),
    Guest(String),
}

pub enum DailyCap {
    User(i64, u32),
    Ip(String, u32),
}

impl DailyCap {
    /// `link_quota.owner` and the cap.
    fn key(&self) -> (String, u32) {
        match self {
            Self::User(id, max) => (format!("u:{id}"), *max),
            Self::Ip(key, max) => (format!("ip:{key}"), *max),
        }
    }
}

const COLS: &str = "id, user_id, slug, url, click_count, permanent_at IS NOT NULL, blocked_at IS NOT NULL, created_at, updated_at";

fn from_row(r: &libsql::Row) -> Result<Link, AppError> {
    Ok(Link {
        id: r.get(0)?,
        user_id: r.get(1)?,
        slug: r.get(2)?,
        url: r.get(3)?,
        click_count: r.get(4)?,
        permanent: r.get::<i64>(5)? != 0,
        blocked: r.get::<i64>(6)? != 0,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

pub async fn find_link(db: &Db, id: i64) -> Result<Option<Link>, AppError> {
    let conn = db.conn().await?;
    let mut rows = conn
        .query(&format!("SELECT {COLS} FROM links WHERE id = ?1"), [id])
        .await?;
    rows.next().await?.as_ref().map(from_row).transpose()
}

pub async fn find_link_by_slug(db: &Db, slug: &str) -> Result<Option<Link>, AppError> {
    let conn = db.conn().await?;
    let mut rows = conn
        .query(&format!("SELECT {COLS} FROM links WHERE slug = ?1"), [slug])
        .await?;
    rows.next().await?.as_ref().map(from_row).transpose()
}

/// Inside a transaction this reads the primary.
pub async fn slug_retired(conn: &libsql::Connection, slug: &str) -> libsql::Result<bool> {
    let mut rows = conn
        .query("SELECT 1 FROM retired_slugs WHERE slug = ?1", [slug])
        .await?;
    Ok(rows.next().await?.is_some())
}

/// Returns the new link, or None when every slug tried was taken or retired.
async fn insert_link(
    conn: &libsql::Connection,
    owner: &Owner,
    url: &url::Url,
    slug: Option<&str>,
    permanent: bool,
    limit: Option<DailyCap>,
) -> Result<Option<Link>, AppError> {
    // Only a daily limit needs a transaction (counter atomic with the insert); else the insert autocommits.
    let tx = match limit {
        Some(cap) => {
            let tx = conn.transaction().await?;
            // Counts creations, not live rows, so deleting doesn't refund; rolled back if no insert happens.
            let day = today_utc_day();
            let (owner, max) = cap.key();
            let n = tx
                .execute(
                    "INSERT INTO link_quota (owner, epoch_day, created)
                     SELECT ?1, ?2, 1 WHERE ?3 > 0
                     ON CONFLICT (owner, epoch_day)
                     DO UPDATE SET created = created + 1 WHERE created < ?3",
                    libsql::params![owner, day, i64::from(max)],
                )
                .await?;
            if n == 0 {
                tx.rollback().await?;
                return Err(AppError::TooManyRequests(
                    "you've reached today's link limit. Try again tomorrow".into(),
                ));
            }
            Some(tx)
        }
        None => None,
    };
    let (uid, guest) = match owner {
        Owner::User(id) => (Some(*id), None),
        Owner::Guest(h) => (None, Some(h.as_str())),
    };
    let now = now_unix();
    for _ in 0..if slug.is_some() { 1 } else { 5 } {
        let s = slug.map_or_else(|| random_base62(7), str::to_owned);
        let mut rows = tx
            .as_deref()
            .unwrap_or(conn)
            .query(
                &format!(
                    "INSERT INTO links (user_id, guest_key_hash, slug, url, permanent_at, created_at, updated_at)
                     SELECT ?1, ?6, ?2, ?3, ?5, ?4, ?4
                     WHERE NOT EXISTS (SELECT 1 FROM retired_slugs WHERE slug = ?2)
                       AND NOT EXISTS (SELECT 1 FROM links WHERE slug = ?2)
                     RETURNING {COLS}"
                ),
                libsql::params![
                    uid,
                    s.as_str(),
                    url.as_str(),
                    now,
                    permanent.then_some(now),
                    guest
                ],
            )
            .await?;
        let Some(link) = rows.next().await?.as_ref().map(from_row).transpose()? else {
            continue;
        };
        drop(rows);
        if let Some(tx) = tx {
            tx.commit().await?;
        }
        return Ok(Some(link));
    }
    if let Some(tx) = tx {
        tx.rollback().await?;
    }
    Ok(None)
}

/// `POST /links` body; only a `Valid<NewLink>` reaches [`create_link`].
#[derive(Debug, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
#[garde(context(Arc<Config>))]
pub struct NewLink {
    #[garde(custom(validate::destination))]
    pub url: String,
    #[garde(inner(custom(custom_slug)))]
    pub slug: Option<String>,
    #[serde(default)]
    #[garde(skip)]
    pub permanent: bool,
}

/// `PATCH /links/{slug}` body; only a `Valid<LinkEdit>` reaches [`update_link`].
#[derive(Debug, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
#[garde(context(Arc<Config>))]
pub struct LinkEdit {
    #[garde(inner(custom(validate::destination)))]
    pub url: Option<String>,
    #[garde(inner(custom(custom_slug)))]
    pub slug: Option<String>,
    #[garde(skip)]
    pub permanent: Option<bool>,
}

/// Stored in parsed (canonical) form; `validate::destination` already parsed this string.
fn canonical(url: &str) -> url::Url {
    url::Url::parse(url).expect("checked by validate::destination")
}

pub async fn create_link(
    db: &Db,
    config: &Config,
    user: &UserRef,
    link: Valid<NewLink>,
) -> Result<Link, AppError> {
    let limit = config
        .max_links_per_user_per_day
        .filter(|_| !user.is_admin)
        .map(|m| DailyCap::User(user.id, m));
    create_owned(db, &Owner::User(user.id), link, false, limit).await
}

/// `restricted` (guests, unverified users): random slug, temporary only.
pub async fn create_owned(
    db: &Db,
    owner: &Owner,
    link: Valid<NewLink>,
    restricted: bool,
    limit: Option<DailyCap>,
) -> Result<Link, AppError> {
    let NewLink {
        url,
        slug,
        permanent,
    } = link.into_inner();
    if restricted {
        let (choose, perm) = match owner {
            Owner::Guest(_) => (
                "sign in to choose a slug",
                "sign in to make a link permanent",
            ),
            Owner::User(_) => (
                "verify your email to choose a slug",
                "verify your email to make a link permanent",
            ),
        };
        if slug.is_some() {
            return Err(AppError::Forbidden(choose.into()));
        }
        if permanent {
            return Err(AppError::Forbidden(perm.into()));
        }
    }
    let (url, slug) = (canonical(&url), slug.as_deref());
    let conn = db.conn().await?;
    match (
        insert_link(conn.writer(), owner, &url, slug, permanent, limit).await?,
        slug,
    ) {
        (Some(link), _) => Ok(link),
        (None, Some(_)) => Err(AppError::Conflict(SLUG_TAKEN.into())),
        (None, None) => Err(AppError::Internal(anyhow::anyhow!(
            "could not generate a unique slug"
        ))),
    }
}

/// Substring pattern for `LIKE ? ESCAPE '\\'`.
pub fn like_pattern(q: &str) -> String {
    let q = q
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{q}%")
}

pub enum LinkScope<'a> {
    All,
    User(i64),
    GuestKey(&'a str),
    AnyGuest,
    AnyUser,
}

pub async fn list_links(
    db: &Db,
    scope: LinkScope<'_>,
    query: Option<&str>,
    cursor: Option<i64>,
    limit: u32,
) -> Result<Vec<Link>, AppError> {
    let conn = db.conn().await?;
    let like = query.map(like_pattern);
    // Spelled out per case so the owner filter can use idx_links_user_id.
    let (owner_filter, key) = match scope {
        LinkScope::User(id) => ("user_id = ?1", libsql::Value::Integer(id)),
        LinkScope::GuestKey(h) => ("guest_key_hash = ?1", libsql::Value::Text(h.into())),
        LinkScope::AnyGuest => ("user_id IS NULL AND ?1 IS NULL", libsql::Value::Null),
        LinkScope::AnyUser => ("user_id IS NOT NULL AND ?1 IS NULL", libsql::Value::Null),
        LinkScope::All => ("?1 IS NULL", libsql::Value::Null),
    };
    let mut rows = conn
        .query(
            &format!(
                "SELECT {COLS} FROM links
                 WHERE {owner_filter}
                   AND (?2 IS NULL OR id < ?2)
                   AND (?3 IS NULL OR slug LIKE ?3 ESCAPE '\\' OR url LIKE ?3 ESCAPE '\\')
                 ORDER BY id DESC LIMIT ?4"
            ),
            libsql::params![key, cursor, like, i64::from(limit)],
        )
        .await?;
    let mut out = vec![];
    while let Some(r) = rows.next().await? {
        out.push(from_row(&r)?);
    }
    Ok(out)
}

pub async fn update_link(
    db: &Db,
    id: i64,
    edit: Valid<LinkEdit>,
) -> Result<(Link, String), AppError> {
    let LinkEdit {
        url,
        slug,
        permanent,
    } = edit.into_inner();
    let (url, slug) = (url.as_deref().map(canonical), slug.as_deref());
    let old = find_link(db, id).await?.ok_or(AppError::NotFound)?;
    let frozen = url.is_some() || slug.is_some() || permanent == Some(false);
    let make_permanent = permanent == Some(true);
    // One statement: the rules are checked on the primary, and `slug = old` makes a concurrent rename a conflict.
    let conn = db.conn().await?;
    let mut rows = conn
        .query(
            &format!(
                "UPDATE links SET url = COALESCE(?1, url), slug = COALESCE(?2, slug), updated_at = ?3,
                     permanent_at = CASE WHEN ?5 THEN COALESCE(permanent_at, ?3) ELSE permanent_at END
                 WHERE id = ?4 AND slug = ?6
                   AND (permanent_at IS NULL OR NOT ?7)
                   AND (user_id IS NOT NULL OR NOT ?5)
                   AND (blocked_at IS NULL OR ?2 IS NULL OR ?2 = ?6)
                   AND NOT EXISTS (SELECT 1 FROM retired_slugs r WHERE r.slug = COALESCE(?2, ?6))
                   AND NOT EXISTS (SELECT 1 FROM links o WHERE o.slug = COALESCE(?2, ?6) AND o.id != ?4)
                 RETURNING {COLS}"
            ),
            libsql::params![
                url.as_ref().map(url::Url::as_str),
                slug,
                now_unix(),
                id,
                make_permanent,
                old.slug.as_str(),
                frozen
            ],
        )
        .await?;
    let new = rows.next().await?.as_ref().map(from_row).transpose()?;
    // Autocommit finishes when the statement is drained; surface its error here.
    while rows.next().await?.is_some() {}
    drop(rows);
    drop(conn);
    if let Some(new) = new {
        return Ok((new, old.slug));
    }
    // Refused: say why, from the replica.
    let cur = find_link(db, id).await?.ok_or(AppError::NotFound)?;
    Err(if cur.slug != old.slug {
        AppError::Conflict("this link changed meanwhile. Try again".into())
    } else if cur.permanent && (url.is_some() || slug.is_some()) {
        AppError::Conflict("permanent links cannot be edited".into())
    } else if cur.permanent && permanent == Some(false) {
        AppError::Conflict("permanent links cannot be made temporary".into())
    } else if cur.blocked && slug.is_some_and(|s| s != old.slug) {
        // Renaming would free the blocked slug for reuse; it stays put, answering 451.
        AppError::Conflict("slug can't change while the link is blocked".into())
    } else if cur.user_id.is_none() && make_permanent {
        AppError::Unprocessable("guest links can't be permanent".into())
    } else if slug.is_some() {
        AppError::Conflict(SLUG_TAKEN.into())
    } else {
        AppError::Conflict("this link changed meanwhile. Try again".into())
    })
}

pub async fn set_blocked(db: &Db, link_id: i64, blocked: bool) -> Result<Option<String>, AppError> {
    let conn = db.conn().await?;
    let now = now_unix();
    let mut rows = conn
        .query(
            "UPDATE links SET blocked_at = ?2, updated_at = ?3 WHERE id = ?1 RETURNING slug",
            libsql::params![link_id, blocked.then_some(now), now],
        )
        .await?;
    let slug = rows.next().await?.map(|r| r.get(0)).transpose()?;
    while rows.next().await?.is_some() {}
    Ok(slug)
}

pub async fn list_retired(
    db: &Db,
    query: Option<&str>,
    cursor: Option<i64>,
    limit: u32,
) -> Result<Vec<(i64, String, i64)>, AppError> {
    let conn = db.conn().await?;
    let mut rows = conn
        .query(
            "SELECT rowid, slug, retired_at FROM retired_slugs
             WHERE (?1 IS NULL OR rowid < ?1)
               AND (?2 IS NULL OR slug LIKE ?2 ESCAPE '\\')
             ORDER BY rowid DESC LIMIT ?3",
            libsql::params![cursor, query.map(like_pattern), i64::from(limit)],
        )
        .await?;
    let mut out = vec![];
    while let Some(r) = rows.next().await? {
        out.push((r.get(0)?, r.get(1)?, r.get(2)?));
    }
    Ok(out)
}

/// False when the slug was not retired.
pub async fn release_slug(db: &Db, slug: &str) -> Result<bool, AppError> {
    let conn = db.conn().await?;
    let mut rows = conn
        .query(
            "DELETE FROM retired_slugs WHERE slug = ?1 RETURNING 1",
            [slug],
        )
        .await?;
    let found = rows.next().await?.is_some();
    while rows.next().await?.is_some() {}
    Ok(found)
}

pub async fn delete_link(db: &Db, id: i64) -> Result<Option<String>, AppError> {
    let conn = db.conn().await?;
    // Temporary links in one statement; permanent and blocked ones need the retire in the same transaction.
    let mut rows = conn
        .query(
            "DELETE FROM links WHERE id = ?1 AND permanent_at IS NULL AND blocked_at IS NULL RETURNING slug",
            [id],
        )
        .await?;
    if let Some(r) = rows.next().await? {
        let slug = r.get(0)?;
        // Autocommit finishes when the statement is drained; surface its error here.
        while rows.next().await?.is_some() {}
        drop(rows);
        return Ok(Some(slug));
    }
    drop(rows);
    let tx = conn.transaction().await?;
    let mut rows = tx
        .query(
            "DELETE FROM links WHERE id = ?1
             RETURNING slug, permanent_at IS NOT NULL OR blocked_at IS NOT NULL",
            [id],
        )
        .await?;
    let found = rows
        .next()
        .await?
        .map(|r| Ok::<_, libsql::Error>((r.get::<String>(0)?, r.get::<i64>(1)?)))
        .transpose()?;
    drop(rows);
    let Some((slug, retire)) = found else {
        tx.rollback().await?;
        return Ok(None);
    };
    if retire != 0 {
        tx.execute(
            "INSERT INTO retired_slugs (slug, retired_at) VALUES (?1, ?2)",
            libsql::params![slug.as_str(), now_unix()],
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Some(slug))
}
