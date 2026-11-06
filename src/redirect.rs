use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};

use crate::{AppState, ClientAddr, cache::CachedSlug, links::slug::is_well_formed_slug};

pub async fn redirect(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    ClientAddr(ip): ClientAddr,
) -> Response {
    let path = uri.path().strip_prefix('/').unwrap_or_default();
    // `/{slug}+` previews the destination; slugs never contain `+`.
    let (slug, preview) = path.strip_suffix('+').map_or((path, false), |s| (s, true));
    if !is_well_formed_slug(slug) {
        return state.frontend.status_page(StatusCode::NOT_FOUND);
    }
    match state.cache.resolve(&state.db, slug).await {
        Ok(CachedSlug::Found { url, permanent, .. }) if preview => {
            state
                .frontend
                .preview(slug, url.to_str().unwrap_or_default(), permanent)
        }
        Ok(CachedSlug::Found {
            link_id,
            url,
            permanent,
        }) => {
            // Link checkers and monitors send HEAD; only a GET is a visit.
            if method == Method::GET {
                state.stats.record(link_id, &headers, ip);
            }
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
