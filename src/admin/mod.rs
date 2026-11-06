use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{delete, get, patch},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    AppState,
    auth::{
        AdminUser, Role, User,
        backend::{COLS, OTHER_ACTIVE_ADMIN, find_user, from_row},
        delete_user, session_store,
    },
    db::now_unix,
    error::{ApiJson, ApiPath, ApiQuery, AppError},
    links::{
        LinkOut, PageQuery, list_page,
        model::{
            LinkScope, find_link, find_link_by_slug, like_pattern, list_retired, release_slug,
            set_blocked,
        },
        paginate,
        slug::is_well_formed_slug,
    },
    validate::{self, Id},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(list_users))
        .route(
            "/admin/users/{id}",
            get(get_user).patch(update_user).delete(remove_user),
        )
        .route("/admin/links", get(list_links))
        .route("/admin/links/{slug}", patch(block_link))
        .route("/admin/retired-slugs", get(list_retired_slugs))
        .route("/admin/retired-slugs/{slug}", delete(release_retired))
}

#[derive(Serialize)]
struct UserOut {
    #[serde(flatten)]
    user: User,
    link_count: i64,
}

async fn list_users(
    State(s): State<AppState>,
    AdminUser(_): AdminUser,
    ApiQuery(q): ApiQuery<PageQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit();
    let like = q.query.as_deref().map(like_pattern);
    let conn = s.db.conn().await?;
    let mut rows = conn
        .query(
            &format!(
                "SELECT {COLS}, (SELECT COUNT(*) FROM links l WHERE l.user_id = users.id) FROM users
                 WHERE (?1 IS NULL OR id < ?1)
                   AND (?2 IS NULL OR email LIKE ?2 ESCAPE '\\' OR name LIKE ?2 ESCAPE '\\')
                 ORDER BY id DESC LIMIT ?3"
            ),
            libsql::params![q.cursor.map(|c| c.0), like, i64::from(limit) + 1],
        )
        .await?;
    let mut users = vec![];
    while let Some(r) = rows.next().await? {
        users.push(UserOut {
            user: from_row(&r)?.0,
            link_count: r.get(8)?,
        });
    }
    let (users, next) = paginate(users, limit, |u| u.user.id);
    Ok(Json(json!({ "users": users, "next_cursor": next })))
}

#[derive(Serialize)]
struct UserDetail {
    #[serde(flatten)]
    user: UserOut,
    click_count: i64,
    /// Linked OAuth providers, e.g. `["github"]`.
    providers: Vec<String>,
}

async fn get_user(
    State(s): State<AppState>,
    AdminUser(_): AdminUser,
    ApiPath(Id(id)): ApiPath<Id>,
) -> Result<Json<UserDetail>, AppError> {
    let conn = s.db.conn().await?;
    let mut rows = conn
        .query(
            &format!(
                "SELECT {COLS}, (SELECT COUNT(*) FROM links l WHERE l.user_id = users.id),
                 (SELECT COALESCE(SUM(click_count), 0) FROM links l WHERE l.user_id = users.id)
                 FROM users WHERE id = ?1"
            ),
            [id],
        )
        .await?;
    let r = rows.next().await?.ok_or(AppError::NotFound)?;
    let (user, link_count, click_count) = (from_row(&r)?.0, r.get(8)?, r.get(9)?);
    drop(rows);
    Ok(Json(UserDetail {
        user: UserOut { user, link_count },
        click_count,
        providers: crate::auth::oauth::list_providers(&s.db, id).await?,
    }))
}

#[derive(Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
struct UpdateUserBody {
    role: Option<Role>,
    disabled: Option<bool>,
    email_verified: Option<bool>,
}

async fn update_user(
    State(s): State<AppState>,
    AdminUser(me): AdminUser,
    ApiPath(Id(id)): ApiPath<Id>,
    ApiJson(body): ApiJson<UpdateUserBody>,
) -> Result<Json<User>, AppError> {
    let demote = body.role == Some(Role::User);
    let disable = body.disabled == Some(true);
    if id == me.id && (demote || disable) {
        return Err(AppError::Conflict(
            "you can't demote or disable yourself".into(),
        ));
    }
    if let Some(d) = body.disabled {
        set_user_disabled(&s, id, d).await?;
    }
    let conn = s.db.conn().await?;
    let mut rows = conn
        .query(
            &format!(
                "UPDATE users SET
                 role = COALESCE(?2, role),
                 email_verified_at = CASE WHEN ?3 IS NULL THEN email_verified_at
                     WHEN ?3 THEN COALESCE(email_verified_at, ?4) ELSE NULL END
             WHERE id = ?1 AND (NOT ?5 OR role != 'admin' OR disabled_at IS NOT NULL OR {OTHER_ACTIVE_ADMIN})
             RETURNING {COLS}"
            ),
            libsql::params![
                id,
                body.role.map(Role::as_str),
                body.email_verified,
                now_unix(),
                demote
            ],
        )
        .await?;
    let user = rows
        .next()
        .await?
        .as_ref()
        .map(from_row)
        .transpose()?
        .map(|u| u.0);
    drop(rows);
    let Some(user) = user else {
        // Nothing changed; tell a missing user from the last-admin guard via the replica.
        return Err(last_admin_or_missing(&s.db, id).await);
    };
    tracing::info!(
        admin_id = me.id,
        user_id = id,
        role = ?body.role.map(Role::as_str),
        disabled = ?body.disabled,
        email_verified = ?body.email_verified,
        "user updated"
    );
    Ok(Json(user))
}

/// Why a guarded user update matched nothing: the user is gone, or they're the last active admin.
async fn last_admin_or_missing(db: &crate::db::Db, id: i64) -> AppError {
    match find_user(db, id).await {
        Ok(Some(_)) => AppError::Conflict(
            "can't demote or disable the last admin. Make another user an admin first".into(),
        ),
        Ok(None) => AppError::NotFound,
        Err(e) => e,
    }
}

/// Disabling revokes sessions; the last active admin can't be disabled.
pub async fn set_user_disabled(s: &AppState, id: i64, disabled: bool) -> Result<(), AppError> {
    let conn = s.db.conn().await?;
    let mut rows = conn
        .query(
            &format!(
                "UPDATE users SET disabled_at = CASE WHEN ?2 THEN COALESCE(disabled_at, ?3) ELSE NULL END
             WHERE id = ?1 AND (NOT ?2 OR role != 'admin' OR disabled_at IS NOT NULL OR {OTHER_ACTIVE_ADMIN})
             RETURNING 1"
            ),
            libsql::params![id, disabled, now_unix()],
        )
        .await?;
    let changed = rows.next().await?.is_some();
    while rows.next().await?.is_some() {}
    drop(rows);
    if !changed {
        return Err(last_admin_or_missing(&s.db, id).await);
    }
    if disabled {
        session_store::delete_user_sessions(conn.writer(), id).await?;
    }
    // Their links stop or resume redirecting; only those cache entries change.
    let mut rows = conn
        .query("SELECT slug FROM links WHERE user_id = ?1", [id])
        .await?;
    let mut slugs = vec![];
    while let Some(r) = rows.next().await? {
        slugs.push(r.get::<String>(0)?);
    }
    drop(rows);
    // Records the write first, so redirects after the evict catch the replica up.
    drop(conn);
    for slug in &slugs {
        s.cache.evict(slug);
    }
    Ok(())
}

#[derive(Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
struct BlockBody {
    blocked: bool,
}

async fn block_link(
    State(s): State<AppState>,
    AdminUser(me): AdminUser,
    ApiPath(slug): ApiPath<String>,
    origin: crate::ShortOrigin,
    ApiJson(body): ApiJson<BlockBody>,
) -> Result<Json<LinkOut>, AppError> {
    let id = find_link_by_slug(&s.db, &slug)
        .await?
        .ok_or(AppError::NotFound)?
        .id;
    set_blocked(&s.db, id, body.blocked)
        .await?
        .ok_or(AppError::NotFound)?;
    s.cache.evict(&slug);
    tracing::info!(
        admin_id = me.id,
        link_id = id,
        slug,
        blocked = body.blocked,
        "link block changed"
    );
    let link = find_link(&s.db, id).await?.ok_or(AppError::NotFound)?;
    Ok(Json(LinkOut::new(&origin, link)))
}

async fn remove_user(
    State(s): State<AppState>,
    AdminUser(me): AdminUser,
    ApiPath(Id(id)): ApiPath<Id>,
) -> Result<StatusCode, AppError> {
    if id == me.id {
        return Err(AppError::Conflict(
            "you can't delete your own account here. Use Settings instead".into(),
        ));
    }
    let slugs = delete_user(&s.db, id).await?;
    for slug in &slugs {
        s.cache.evict(slug);
    }
    tracing::info!(
        admin_id = me.id,
        user_id = id,
        links = slugs.len(),
        "user deleted"
    );
    Ok(StatusCode::NO_CONTENT)
}

// No `#[serde(flatten)]`: serde_urlencoded can't parse numbers through it.
#[derive(Deserialize, garde::Validate)]
struct LinksQuery {
    #[serde(default, deserialize_with = "validate::de_query")]
    #[garde(inner(custom(validate::search_query)))]
    query: Option<String>,
    #[garde(skip)]
    cursor: Option<Id>,
    #[garde(skip)]
    limit: Option<u32>,
    #[garde(skip)]
    user_id: Option<Id>,
    #[garde(skip)]
    guest: Option<bool>,
}

async fn list_links(
    State(s): State<AppState>,
    AdminUser(_): AdminUser,
    origin: crate::ShortOrigin,
    ApiQuery(q): ApiQuery<LinksQuery>,
) -> Result<Json<Value>, AppError> {
    let q = q.into_inner();
    let page = PageQuery {
        query: q.query,
        cursor: q.cursor,
        limit: q.limit,
    };
    let scope = match (q.user_id, q.guest) {
        (Some(_), Some(true)) => {
            return Err(AppError::Unprocessable(
                "choose either guest or user_id".into(),
            ));
        }
        (Some(u), _) => LinkScope::User(u.0),
        (None, Some(true)) => LinkScope::AnyGuest,
        (None, Some(false)) => LinkScope::AnyUser,
        (None, None) => LinkScope::All,
    };
    list_page(&s, &origin, scope, &page).await
}

async fn list_retired_slugs(
    State(s): State<AppState>,
    AdminUser(_): AdminUser,
    ApiQuery(q): ApiQuery<PageQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit();
    let rows = list_retired(&s.db, q.query.as_deref(), q.cursor.map(|c| c.0), limit + 1).await?;
    let (rows, next) = paginate(rows, limit, |r| r.0);
    let slugs: Vec<_> = rows
        .into_iter()
        .map(|(id, slug, retired_at)| json!({ "id": id, "slug": slug, "retired_at": retired_at }))
        .collect();
    Ok(Json(json!({ "slugs": slugs, "next_cursor": next })))
}

async fn release_retired(
    State(s): State<AppState>,
    AdminUser(me): AdminUser,
    ApiPath(slug): ApiPath<String>,
) -> Result<StatusCode, AppError> {
    if !is_well_formed_slug(&slug) || !release_slug(&s.db, &slug).await? {
        return Err(AppError::NotFound);
    }
    s.cache.evict(&slug);
    tracing::info!(admin_id = me.id, slug, "retired slug released");
    Ok(StatusCode::NO_CONTENT)
}
