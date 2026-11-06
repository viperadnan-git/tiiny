pub mod model;
pub mod slug;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState, ClientAddr, ShortOrigin,
    auth::{CurrentUser, backend::AuthSession, email::hash_token, rate_limit_key},
    error::{ApiJson, ApiPath, ApiQuery, AppError},
    guest, stats,
    validate::{self, Id},
};
use model::{DailyCap, Link, Owner};

#[derive(Deserialize, garde::Validate)]
pub struct PageQuery {
    #[serde(default, deserialize_with = "validate::de_query")]
    #[garde(inner(custom(validate::search_query)))]
    pub query: Option<String>,
    #[garde(skip)]
    pub cursor: Option<Id>,
    #[garde(skip)]
    pub limit: Option<u32>,
}

impl PageQuery {
    /// Default 50; out-of-range values are clamped to 1..=100.
    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(50).clamp(1, 100)
    }
}

/// Fetches one extra row to tell whether another page exists.
pub fn paginate<T>(mut items: Vec<T>, limit: u32, id: impl Fn(&T) -> i64) -> (Vec<T>, Option<i64>) {
    if items.len() > limit as usize {
        items.truncate(limit as usize);
        let next = items.last().map(id);
        (items, next)
    } else {
        (items, None)
    }
}

#[derive(Serialize)]
pub struct LinkOut {
    #[serde(flatten)]
    link: Link,
    short_url: String,
}

impl LinkOut {
    pub fn new(origin: &ShortOrigin, link: Link) -> Self {
        Self {
            short_url: origin.short_url(&link.slug),
            link,
        }
    }
}

pub async fn list_page(
    state: &AppState,
    origin: &ShortOrigin,
    scope: model::LinkScope<'_>,
    q: &PageQuery,
) -> Result<Json<serde_json::Value>, AppError> {
    let limit = q.limit();
    let links = model::list_links(
        &state.db,
        scope,
        q.query.as_deref(),
        q.cursor.map(|c| c.0),
        limit + 1,
    )
    .await?;
    let (links, next) = paginate(links, limit, |l| l.id);
    let links: Vec<_> = links.into_iter().map(|l| LinkOut::new(origin, l)).collect();
    Ok(Json(
        serde_json::json!({ "links": links, "next_cursor": next }),
    ))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/links", get(list).post(create))
        .route("/links/guest", get(list_guest))
        .route("/links/{slug}", get(get_one).patch(update).delete(remove))
        .route("/links/{slug}/stats", get(link_stats))
}

/// Non-owners (other than admins) get 404 so slugs in use don't leak.
async fn owned(state: &AppState, user: &crate::auth::User, slug: &str) -> Result<Link, AppError> {
    model::find_link_by_slug(&state.db, slug)
        .await?
        .filter(|l| l.user_id == Some(user.id) || user.is_admin())
        .ok_or(AppError::NotFound)
}

async fn create(
    State(s): State<AppState>,
    auth: AuthSession,
    ClientAddr(ip): ClientAddr,
    headers: HeaderMap,
    origin: ShortOrigin,
    ApiJson(link): ApiJson<model::NewLink>,
) -> Result<Response, AppError> {
    validate::not_this_host(&link.url, origin.host.as_deref())?;
    let c = &s.config;
    let ip_cap = || {
        let key = ip.map_or_else(|| "unknown".into(), |ip| rate_limit_key(ip).to_string());
        DailyCap::Ip(key, c.max_guest_links_per_ip_per_day)
    };
    let mut set_cookie = None;
    let user_id = auth.user.as_ref().map(|u| u.id);
    let created = match auth.user {
        None => {
            if !c.guest_links() {
                return Err(AppError::Unauthorized);
            }
            guest::check(&s.guest_limiter, ip)?;
            let hash = guest::key_hash(c, &headers).unwrap_or_else(|| {
                let (secret, cookie) = guest::new_cookie(c);
                set_cookie = Some(cookie);
                hash_token(&secret)
            });
            model::create_owned(&s.db, &Owner::Guest(hash), link, true, Some(ip_cap())).await?
        }
        Some(u) if s.mailer.is_enabled() && !u.email_verified && !u.is_admin() => {
            if !c.guest_links() {
                return Err(AppError::Forbidden(
                    "verify your email to create links".into(),
                ));
            }
            guest::check(&s.guest_limiter, ip)?;
            model::create_owned(&s.db, &Owner::User(u.id), link, true, Some(ip_cap())).await?
        }
        Some(u) => model::create_link(&s.db, c, &u.as_user_ref(), link).await?,
    };
    // A cached Missing/Retired entry would otherwise hide the new link.
    s.cache.evict(&created.slug);
    tracing::debug!(
        link_id = created.id,
        slug = created.slug,
        user_id,
        "link created"
    );
    let mut res = (StatusCode::CREATED, Json(LinkOut::new(&origin, created))).into_response();
    if let Some(v) = set_cookie {
        res.headers_mut().insert(header::SET_COOKIE, v);
    }
    Ok(res)
}

async fn list_guest(
    State(s): State<AppState>,
    headers: HeaderMap,
    origin: ShortOrigin,
    ApiQuery(q): ApiQuery<PageQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    match guest::key_hash(&s.config, &headers) {
        Some(h) => list_page(&s, &origin, model::LinkScope::GuestKey(&h), &q).await,
        None => Ok(Json(
            serde_json::json!({ "links": [], "next_cursor": null }),
        )),
    }
}

async fn list(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    origin: ShortOrigin,
    ApiQuery(q): ApiQuery<PageQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    list_page(&s, &origin, model::LinkScope::User(user.id), &q).await
}

async fn get_one(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(slug): ApiPath<String>,
    origin: ShortOrigin,
) -> Result<Json<LinkOut>, AppError> {
    let link = owned(&s, &user, &slug).await?;
    Ok(Json(LinkOut::new(&origin, link)))
}

async fn update(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(slug): ApiPath<String>,
    origin: ShortOrigin,
    ApiJson(edit): ApiJson<model::LinkEdit>,
) -> Result<Json<LinkOut>, AppError> {
    if let Some(url) = &edit.url {
        validate::not_this_host(url, origin.host.as_deref())?;
    }
    let current = owned(&s, &user, &slug).await?;
    if current.blocked && !user.is_admin() {
        return Err(AppError::Forbidden(
            "an admin blocked this link, so it can't be changed. You can still delete it".into(),
        ));
    }
    if s.mailer.is_enabled() && !user.email_verified && !user.is_admin() {
        if edit.slug.is_some() {
            return Err(AppError::Forbidden(
                "verify your email to choose a slug".into(),
            ));
        }
        if edit.permanent == Some(true) {
            return Err(AppError::Forbidden(
                "verify your email to make a link permanent".into(),
            ));
        }
    }
    let (link, old_slug) = model::update_link(&s.db, current.id, edit).await?;
    s.cache.evict(&old_slug);
    s.cache.evict(&link.slug);
    tracing::debug!(
        link_id = link.id,
        slug = link.slug,
        old_slug,
        user_id = user.id,
        "link updated"
    );
    Ok(Json(LinkOut::new(&origin, link)))
}

async fn remove(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(slug): ApiPath<String>,
) -> Result<StatusCode, AppError> {
    let id = owned(&s, &user, &slug).await?.id;
    let slug = model::delete_link(&s.db, id)
        .await?
        .ok_or(AppError::NotFound)?;
    s.cache.evict(&slug);
    tracing::debug!(link_id = id, slug, user_id = user.id, "link deleted");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, garde::Validate)]
#[garde(allow_unvalidated)]
struct StatsQuery {
    range: Option<String>,
}

async fn link_stats(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(slug): ApiPath<String>,
    ApiQuery(q): ApiQuery<StatsQuery>,
) -> Result<Json<stats::LinkStats>, AppError> {
    let id = owned(&s, &user, &slug).await?.id;
    let range = q.range.as_deref().unwrap_or("30d").parse()?;
    Ok(Json(stats::link_stats(&s.db, id, range).await?))
}
