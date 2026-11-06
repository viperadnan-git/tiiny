use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};

use rust_embed::Embed;

use crate::{AppState, redirect};

/// Debug builds read `web/build` from disk at runtime; release builds embed it.
#[derive(Embed)]
#[folder = "web/build"]
pub struct Assets;

/// Written into prerendered pages and robots.txt at build time; each instance's domain is unknown then.
pub const BASE_URL_PLACEHOLDER: &str = "%%TIINY_BASE_URL%%";

const SPA_PREFIXES: &[&str] = &["/login", "/signup", "/auth", "/dashboard", "/admin"];
const SPA_SHELL: &str = "200.html";
const STATUS_PAGES: &[&str] = &["404.html", "410.html", "blocked.html", PREVIEW_PAGE];
const PREVIEW_PAGE: &str = "preview.html";

/// Embedded files with the base-URL placeholder replaced, the prerendered pages and the sitemap, built once at startup.
#[derive(Clone)]
pub struct Frontend {
    substituted: Arc<HashMap<String, Bytes>>,
    /// Route → file for every top-level prerendered page (`index.html` → `/`).
    pages: Arc<BTreeMap<String, String>>,
    /// None without `TIINY_BASE_URL`.
    sitemap: Option<Bytes>,
}

impl Frontend {
    pub fn new(origin: &str) -> Self {
        let substituted = Assets::iter()
            .filter_map(|path| {
                let f = Assets::get(&path)?;
                let text = std::str::from_utf8(&f.data).ok()?;
                text.contains(BASE_URL_PLACEHOLDER).then(|| {
                    let mut body = text.replace(BASE_URL_PLACEHOLDER, origin);
                    // No sitemap without an origin: its URLs must be absolute.
                    if origin.is_empty() && path == "robots.txt" {
                        body = body
                            .lines()
                            .filter(|l| !l.starts_with("Sitemap:"))
                            .map(|l| format!("{l}\n"))
                            .collect();
                    }
                    (path.into_owned(), Bytes::from(body))
                })
            })
            .collect();
        let pages: BTreeMap<String, String> = Assets::iter()
            .filter(|p| {
                !p.contains('/')
                    && p.ends_with(".html")
                    && *p != SPA_SHELL
                    && !STATUS_PAGES.contains(&&**p)
            })
            .map(|p| {
                let name = p.trim_end_matches(".html");
                let route = if name == "index" {
                    "/".to_owned()
                } else {
                    format!("/{name}")
                };
                (route, p.into_owned())
            })
            .collect();
        let urls: String = pages
            .keys()
            .map(|r| format!("\t<url><loc>{origin}{r}</loc></url>\n"))
            .collect();
        let sitemap = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n{urls}</urlset>\n"
        );
        Self {
            substituted: Arc::new(substituted),
            pages: Arc::new(pages),
            sitemap: (!origin.is_empty()).then(|| Bytes::from(sitemap)),
        }
    }

    /// Routes of the prerendered pages; also the sitemap's entries.
    pub fn page_routes(&self) -> impl Iterator<Item = &str> {
        self.pages.keys().map(String::as_str)
    }

    fn sitemap(&self) -> Option<Response> {
        let body = self.sitemap.clone()?;
        Some(
            (
                [
                    (header::CONTENT_TYPE, "application/xml"),
                    (header::CACHE_CONTROL, "no-cache"),
                    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                ],
                body,
            )
                .into_response(),
        )
    }

    fn body(&self, path: &str) -> Option<Bytes> {
        if let Some(b) = self.substituted.get(path) {
            return Some(b.clone());
        }
        Some(match Assets::get(path)?.data {
            Cow::Borrowed(b) => Bytes::from_static(b),
            Cow::Owned(v) => Bytes::from(v),
        })
    }

    fn file(&self, path: &str, status: StatusCode) -> Option<Response> {
        let f = Assets::get(path)?;
        let cache = HeaderValue::from_static(if path.starts_with("_app/immutable/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        });
        let mime = f.metadata.mimetype();
        let content_type = HeaderValue::from_str(mime).ok()?;
        let body = self.body(path)?;
        let mut r = (
            status,
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, cache),
            ],
            body,
        )
            .into_response();
        let h = r.headers_mut();
        h.insert(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        );
        if mime.starts_with("text/html") {
            h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
            h.insert(
                header::REFERRER_POLICY,
                HeaderValue::from_static("strict-origin-when-cross-origin"),
            );
        }
        Some(r)
    }

    /// `/{slug}+`: where the link goes, without redirecting. Plain text if the frontend wasn't built.
    pub fn preview(&self, slug: &str, url: &str, permanent: bool) -> Response {
        let host = url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default();
        let code = if permanent { "301" } else { "302" };
        let Some(mut r) = self.file(PREVIEW_PAGE, StatusCode::OK) else {
            return format!("{slug} goes to {url}\n").into_response();
        };
        let html = String::from_utf8_lossy(&self.body(PREVIEW_PAGE).unwrap_or_default())
            .replace("%%TIINY_PREVIEW_SLUG%%", &escape(slug))
            .replace("%%TIINY_PREVIEW_URL%%", &escape(url))
            .replace("%%TIINY_PREVIEW_HOST%%", &escape(&host))
            .replace("%%TIINY_PREVIEW_CODE%%", code);
        r.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-cache"),
        );
        *r.body_mut() = html.into();
        r
    }

    /// HTML page for 404 / 410 / 451; plain text if the frontend wasn't built.
    pub fn status_page(&self, status: StatusCode) -> Response {
        let name = match status {
            StatusCode::GONE => "410.html",
            StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS => "blocked.html",
            _ => "404.html",
        };
        self.file(name, status).unwrap_or_else(|| {
            (status, status.canonical_reason().unwrap_or_default()).into_response()
        })
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn is_spa_route(path: &str) -> bool {
    SPA_PREFIXES.iter().any(|p| {
        path.strip_prefix(p)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

pub async fn fallback(
    state: State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    addr: crate::ClientAddr,
) -> Response {
    let fe = &state.frontend;
    let path = uri.path();
    if path == "/sitemap.xml"
        && let Some(r) = fe.sitemap()
    {
        return r;
    }
    // HTML is only reachable through its route, the SPA shell or a status page.
    if let Some(r) = path
        .strip_prefix('/')
        .filter(|p| !p.is_empty() && !p.ends_with(".html"))
        .and_then(|p| fe.file(p, StatusCode::OK))
    {
        return r;
    }
    if let Some(page) = fe.pages.get(path) {
        return fe
            .file(page, StatusCode::OK)
            .unwrap_or_else(|| fe.status_page(StatusCode::NOT_FOUND));
    }
    if is_spa_route(path) {
        return fe
            .file(SPA_SHELL, StatusCode::OK)
            .unwrap_or_else(|| fe.status_page(StatusCode::NOT_FOUND));
    }
    redirect::redirect(state, uri, headers, addr).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::links::slug::RESERVED_SLUGS;

    #[test]
    fn page_routes_and_spa_prefixes_are_reserved_slugs() {
        let fe = Frontend::new("https://link.example");
        let routes: Vec<&str> = fe.page_routes().collect();
        assert!(
            routes.contains(&"/") && routes.len() > 1,
            "web/build missing? {routes:?}"
        );
        for route in routes
            .into_iter()
            .filter(|r| *r != "/")
            .chain(SPA_PREFIXES.iter().copied())
        {
            assert!(RESERVED_SLUGS.contains(&&route[1..]), "{route}");
        }
    }
}
