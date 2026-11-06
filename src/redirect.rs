use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};

use crate::{AppState, ClientAddr, cache::CachedSlug, links::slug::is_well_formed_slug};

pub async fn redirect(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    ClientAddr(ip): ClientAddr,
) -> Response {
    let slug = uri.path().strip_prefix('/').unwrap_or_default();
    if !is_well_formed_slug(slug) {
        return state.frontend.status_page(StatusCode::NOT_FOUND);
    }
    match state.cache.resolve(&state.db, slug).await {
        Ok(CachedSlug::Found {
            link_id,
            url,
            permanent,
        }) => {
            state.stats.record(link_id, &headers, ip);
            // A day, browsers only: blocking, deleting or disabling the owner must reach earlier visitors.
            let (status, cache) = if permanent {
                (StatusCode::MOVED_PERMANENTLY, "private, max-age=86400")
            } else {
                (StatusCode::FOUND, "private, no-cache")
            };
            (
                status,
                [
                    (header::LOCATION, url),
                    (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
                ],
            )
                .into_response()
        }
        Ok(CachedSlug::Missing) => state.frontend.status_page(StatusCode::NOT_FOUND),
        Ok(CachedSlug::Retired) => state.frontend.status_page(StatusCode::GONE),
        Ok(CachedSlug::Blocked) => state
            .frontend
            .status_page(StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS),
        Err(e) => {
            tracing::error!(error = %e, "slug lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
