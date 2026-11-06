use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};

use rust_embed::Embed;
use sha2::{Digest, Sha256};

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
/// Filled per request by [`Frontend::preview`], in this order; `preview/+page.svelte` holds each one.
const PREVIEW_TOKENS: [&str; 4] = [
    "%%TIINY_PREVIEW_SLUG%%",
    "%%TIINY_PREVIEW_URL%%",
    "%%TIINY_PREVIEW_HOST%%",
    "%%TIINY_PREVIEW_CODE%%",
];
/// Literal pieces of `preview.html`, each followed by the index of the token after it.
type Template = Vec<(String, Option<usize>)>;

/// Embedded files with the base-URL placeholder replaced, the prerendered pages and the sitemap, built once at startup.
#[derive(Clone)]
pub struct Frontend {
    /// Files holding the base-URL placeholder, filled in, with an ETag of the result.
    substituted: Arc<HashMap<String, (Bytes, HeaderValue)>>,
    /// `preview.html` split at its tokens; None if it's missing one (logged at startup) or wasn't built.
    preview: Option<Arc<Template>>,
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
                    let tag = etag(&Sha256::digest(body.as_bytes()));
                    (path.into_owned(), (Bytes::from(body), tag))
                })
            })
            .collect::<HashMap<_, _>>();
        let preview = substituted
            .get(PREVIEW_PAGE)
            .map(|(b, _)| String::from_utf8_lossy(b).into_owned())
            .or_else(|| {
                Assets::get(PREVIEW_PAGE).map(|f| String::from_utf8_lossy(&f.data).into_owned())
            })
            .and_then(|html| {
                let t = split_tokens(&html);
                if t.is_none() {
                    tracing::error!(
                        "preview.html is missing a preview token; /slug+ answers in plain text"
                    );
                }
                t
            });
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
            preview: preview.map(Arc::new),
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

    fn file(&self, path: &str, status: StatusCode) -> Option<Response> {
        let f = Assets::get(path)?;
        let (body, tag) = match self.substituted.get(path) {
            Some((b, tag)) => (b.clone(), tag.clone()),
            None => (
                match f.data {
                    Cow::Borrowed(b) => Bytes::from_static(b),
                    Cow::Owned(v) => Bytes::from(v),
                },
                etag(&f.metadata.sha256_hash()),
            ),
        };
        let cache = if path.starts_with("_app/immutable/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        let mut r = respond(status, f.metadata.mimetype(), cache, body)?;
        r.headers_mut().insert(header::ETAG, tag);
        Some(r)
    }

    /// `/{slug}+`: where the link goes, without redirecting. Plain text if the frontend wasn't built.
    pub fn preview(&self, slug: &str, url: &str, permanent: bool) -> Response {
        let host = url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default();
        let code = if permanent { "301" } else { "302" };
        let plain = || format!("{slug} goes to {url}\n").into_response();
        let Some(parts) = &self.preview else {
            return plain();
        };
        let values = [escape(slug), escape(url), escape(&host), code.to_owned()];
        let mut html = String::with_capacity(
            parts.iter().map(|(l, _)| l.len()).sum::<usize>() + 4 * url.len(),
        );
        for (lit, token) in parts.iter() {
            html.push_str(lit);
            if let Some(i) = token {
                html.push_str(&values[*i]);
            }
        }
        respond(
            StatusCode::OK,
            "text/html",
            "private, no-cache",
            html.into(),
        )
        .unwrap_or_else(plain)
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

/// Content type, cache policy and the security headers every page shares.
fn respond(status: StatusCode, mime: &str, cache: &'static str, body: Bytes) -> Option<Response> {
    let mut r = (
        status,
        [
            (header::CONTENT_TYPE, HeaderValue::from_str(mime).ok()?),
            (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
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

/// Weak: compression changes the bytes, not the content.
fn etag(hash: &[u8]) -> HeaderValue {
    use std::fmt::Write;
    let hex = hash[..16]
        .iter()
        .fold(String::with_capacity(32), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        });
    HeaderValue::from_str(&format!("W/\"{hex}\"")).expect("hex is a valid header value")
}

/// None if a token never appears.
fn split_tokens(html: &str) -> Option<Template> {
    let mut parts = Vec::new();
    let mut seen = [false; PREVIEW_TOKENS.len()];
    let mut rest = html;
    while let Some((at, i)) = PREVIEW_TOKENS
        .iter()
        .enumerate()
        .filter_map(|(i, t)| rest.find(t).map(|at| (at, i)))
        .min()
    {
        parts.push((rest[..at].to_owned(), Some(i)));
        seen[i] = true;
        rest = &rest[at + PREVIEW_TOKENS[i].len()..];
    }
    parts.push((rest.to_owned(), None));
    seen.iter().all(|&s| s).then_some(parts)
}

/// `304` when the browser already holds this version.
fn not_modified(req: &HeaderMap, r: Response) -> Response {
    let fresh = r.status() == StatusCode::OK
        && r.headers().get(header::ETAG).is_some_and(|tag| {
            req.get_all(header::IF_NONE_MATCH)
                .iter()
                .filter_map(|v| v.to_str().ok())
                .flat_map(|v| v.split(','))
                .any(|v| {
                    let v = v.trim();
                    v == "*"
                        || v.trim_start_matches("W/")
                            == tag.to_str().unwrap_or("").trim_start_matches("W/")
                })
        });
    if !fresh {
        return r;
    }
    let mut out = StatusCode::NOT_MODIFIED.into_response();
    for name in [header::ETAG, header::CACHE_CONTROL] {
        if let Some(v) = r.headers().get(&name) {
            out.headers_mut().insert(name, v.clone());
        }
    }
    out
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
    method: Method,
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
        return not_modified(&headers, r);
    }
    if let Some(page) = fe.pages.get(path) {
        return fe.file(page, StatusCode::OK).map_or_else(
            || fe.status_page(StatusCode::NOT_FOUND),
            |r| not_modified(&headers, r),
        );
    }
    if is_spa_route(path) {
        return fe.file(SPA_SHELL, StatusCode::OK).map_or_else(
            || fe.status_page(StatusCode::NOT_FOUND),
            |r| not_modified(&headers, r),
        );
    }
    redirect::redirect(state, method, uri, headers, addr).await
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

    #[test]
    fn preview_tokens_split_and_refill() {
        let html = "<a href=\"%%TIINY_PREVIEW_URL%%\">%%TIINY_PREVIEW_HOST%%</a> /%%TIINY_PREVIEW_SLUG%% %%TIINY_PREVIEW_CODE%% %%TIINY_PREVIEW_URL%%";
        let parts = split_tokens(html).unwrap();
        let filled: String = parts
            .iter()
            .map(|(l, t)| format!("{l}{}", t.map_or("", |i| ["s", "u", "h", "c"][i])))
            .collect();
        assert_eq!(filled, "<a href=\"u\">h</a> /s c u");
        assert!(split_tokens("no %%TIINY_PREVIEW_URL%% only").is_none());
        let built = Frontend::new("https://link.example");
        assert!(
            built.preview.is_some(),
            "web/build/preview.html lacks a token"
        );
    }

    #[test]
    fn etag_match_gives_304() {
        let fe = Frontend::new("https://link.example");
        let r = fe.file("index.html", StatusCode::OK).unwrap();
        let tag = r.headers()[header::ETAG].clone();
        assert!(tag.to_str().unwrap().starts_with("W/\""));
        let mut req = HeaderMap::new();
        req.insert(header::IF_NONE_MATCH, tag.clone());
        assert_eq!(not_modified(&req, r).status(), StatusCode::NOT_MODIFIED);
        let other =
            HeaderMap::from_iter([(header::IF_NONE_MATCH, HeaderValue::from_static("W/\"0\""))]);
        let r = fe.file("index.html", StatusCode::OK).unwrap();
        assert_eq!(not_modified(&other, r).status(), StatusCode::OK);
        // The filled-in origin is part of the tag, so a new TIINY_BASE_URL invalidates caches.
        let elsewhere = Frontend::new("https://other.example");
        assert_ne!(
            elsewhere
                .file("index.html", StatusCode::OK)
                .unwrap()
                .headers()[header::ETAG],
            tag
        );
    }
}
