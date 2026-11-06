use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    db::{Db, now_unix},
    error::AppError,
    links::model::like_pattern,
    validate,
};

#[derive(Debug, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct NewReport {
    #[garde(skip)]
    pub slug: String,
    #[garde(custom(validate::report_reason))]
    pub reason: String,
    #[serde(default, deserialize_with = "de_details")]
    #[garde(inner(custom(validate::report_details)))]
    pub details: Option<String>,
    #[serde(default, deserialize_with = "de_contact")]
    #[garde(inner(custom(validate::report_contact_email)))]
    pub contact_email: Option<String>,
}

fn de_details<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(d)?
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty()))
}

fn de_contact<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(d)?
        .map(|s| validate::normalize_email(&s))
        .filter(|s| !s.is_empty()))
}

/// Copies the link's current slug and url; false when no link matches or its owner is disabled.
pub async fn insert(db: &Db, slug: &str, r: &NewReport) -> Result<bool, AppError> {
    let n = db
        .conn()
        .await?
        .execute(
            "INSERT INTO abuse_reports (link_id, slug, url, reason, details, contact_email, created_at) \
             SELECT l.id, l.slug, l.url, ?2, ?3, ?4, ?5 FROM links l LEFT JOIN users u ON u.id = l.user_id \
             WHERE l.slug = ?1 AND u.disabled_at IS NULL",
            libsql::params![slug, &*r.reason, r.details.clone(), r.contact_email.clone(), now_unix()],
        )
        .await?;
    Ok(n > 0)
}

#[derive(Serialize)]
pub struct ReportLink {
    pub id: i64,
    pub slug: String,
    pub url: String,
    pub owner_email: Option<String>,
    pub blocked: bool,
    pub click_count: i64,
}

#[derive(Serialize)]
pub struct ReportRow {
    pub id: i64,
    pub slug: String,
    pub url: String,
    pub reason: String,
    pub details: Option<String>,
    pub contact_email: Option<String>,
    pub status: String,
    pub action: Option<String>,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
    pub link: Option<ReportLink>,
    pub open_for_link: i64,
    /// What reopening an actioned report undoes.
    pub blocked_link: bool,
    pub disabled_owner: bool,
}

pub async fn list(
    db: &Db,
    status: &str,
    query: Option<&str>,
    cursor: Option<i64>,
    limit: i64,
) -> Result<Vec<ReportRow>, AppError> {
    let conn = db.conn().await?;
    let mut rows = conn
        .query(
            "SELECT r.id, r.slug, r.url, r.reason, r.details, r.contact_email, r.status, r.action,
                    r.created_at, r.resolved_at, l.id, l.slug, l.url, u.email, l.blocked_at IS NOT NULL,
                    l.click_count,
                    (SELECT COUNT(*) FROM abuse_reports o WHERE o.link_id = r.link_id AND o.status = 'open'),
                    r.blocked_link, r.disabled_owner
             FROM abuse_reports r
             LEFT JOIN links l ON l.id = r.link_id
             LEFT JOIN users u ON u.id = l.user_id
             WHERE r.status = ?1 AND (?2 IS NULL OR r.id < ?2)
               AND (?4 IS NULL OR r.slug LIKE ?4 ESCAPE '\\' OR r.url LIKE ?4 ESCAPE '\\'
                    OR r.details LIKE ?4 ESCAPE '\\' OR r.contact_email LIKE ?4 ESCAPE '\\'
                    OR u.email LIKE ?4 ESCAPE '\\')
             ORDER BY r.id DESC LIMIT ?3",
            libsql::params![status, cursor, limit, query.map(like_pattern)],
        )
        .await?;
    let mut out = vec![];
    while let Some(r) = rows.next().await? {
        let link = match r.get::<Option<i64>>(10)? {
            Some(id) => Some(ReportLink {
                id,
                slug: r.get(11)?,
                url: r.get(12)?,
                owner_email: r.get(13)?,
                blocked: r.get::<i64>(14)? != 0,
                click_count: r.get(15)?,
            }),
            None => None,
        };
        out.push(ReportRow {
            id: r.get(0)?,
            slug: r.get(1)?,
            url: r.get(2)?,
            reason: r.get(3)?,
            details: r.get(4)?,
            contact_email: r.get(5)?,
            status: r.get(6)?,
            action: r.get(7)?,
            created_at: r.get(8)?,
            resolved_at: r.get(9)?,
            link,
            open_for_link: r.get(16)?,
            blocked_link: r.get::<i64>(17)? != 0,
            disabled_owner: r.get::<i64>(18)? != 0,
        });
    }
    Ok(out)
}

pub async fn open_count(db: &Db) -> Result<i64, AppError> {
    let conn = db.conn().await?;
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM abuse_reports WHERE status = 'open'",
            (),
        )
        .await?;
    Ok(rows.next().await?.map_or(Ok(0), |r| r.get(0))?)
}

/// (link_id, status, owner id) of a report.
pub struct Found {
    pub link_id: Option<i64>,
    pub status: String,
    pub owner: Option<i64>,
    pub action: Option<String>,
    pub resolved_at: Option<i64>,
    /// The action blocked the link / disabled the owner (it wasn't already).
    pub blocked_link: bool,
    pub disabled_owner: bool,
}

pub async fn find(db: &Db, id: i64) -> Result<Option<Found>, AppError> {
    let conn = db.conn().await?;
    let mut rows = conn
        .query(
            "SELECT r.link_id, r.status, l.user_id, r.action, r.resolved_at, r.blocked_link, r.disabled_owner
             FROM abuse_reports r LEFT JOIN links l ON l.id = r.link_id WHERE r.id = ?1",
            [id],
        )
        .await?;
    let Some(r) = rows.next().await? else {
        return Ok(None);
    };
    Ok(Some(Found {
        link_id: r.get(0)?,
        status: r.get(1)?,
        owner: r.get(2)?,
        action: r.get(3)?,
        resolved_at: r.get(4)?,
        blocked_link: r.get::<i64>(5)? != 0,
        disabled_owner: r.get::<i64>(6)? != 0,
    }))
}

/// Back to open: a dismissed report alone, an actioned one with every report its action closed
/// (same link, action and time). Guarded on the state `r` was read in; 0 when it changed.
pub async fn reopen(db: &Db, id: i64, r: &Found) -> Result<u64, AppError> {
    let conn = db.conn().await?;
    let set = "SET status = 'open', action = NULL, resolved_by = NULL, resolved_at = NULL, \
               blocked_link = 0, disabled_owner = 0";
    Ok(if r.status == "dismissed" {
        conn.execute(
            &format!("UPDATE abuse_reports {set} WHERE id = ?1 AND status = 'dismissed'"),
            [id],
        )
        .await?
    } else {
        conn.execute(
            &format!(
                "UPDATE abuse_reports {set}
                 WHERE link_id = ?1 AND status = 'actioned' AND action = ?2 AND resolved_at IS ?3"
            ),
            libsql::params![r.link_id, r.action.as_deref(), r.resolved_at],
        )
        .await?
    })
}

/// Ids of the open reports on a link; read before the link action since deleting nulls `link_id`.
pub async fn open_ids(db: &Db, link_id: i64) -> Result<Vec<i64>, AppError> {
    let conn = db.conn().await?;
    let mut rows = conn
        .query(
            "SELECT id FROM abuse_reports WHERE link_id = ?1 AND status = 'open'",
            [link_id],
        )
        .await?;
    let mut ids = vec![];
    while let Some(r) = rows.next().await? {
        ids.push(r.get(0)?);
    }
    Ok(ids)
}

/// What an action changed on the link and its owner; reopening undoes only these.
#[derive(Default, Clone, Copy)]
pub struct Effects {
    pub blocked_link: bool,
    pub disabled_owner: bool,
}

/// Closes the given reports that are still open; false when none was.
pub async fn close(
    db: &Db,
    ids: &[i64],
    status: &str,
    action: Option<&str>,
    effects: Effects,
    admin: i64,
) -> Result<bool, AppError> {
    let conn = db.conn().await?;
    let changed = conn
        .execute(
            "UPDATE abuse_reports SET status = ?2, action = ?3, resolved_by = ?4, resolved_at = ?5,
                 blocked_link = ?6, disabled_owner = ?7
             WHERE id IN (SELECT value FROM json_each(?1)) AND status = 'open'",
            libsql::params![
                serde_json::to_string(ids).expect("ids serialize"),
                status,
                action,
                admin,
                now_unix(),
                effects.blocked_link,
                effects.disabled_owner
            ],
        )
        .await?;
    Ok(changed > 0)
}
