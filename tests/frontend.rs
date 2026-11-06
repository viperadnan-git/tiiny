mod common;

use axum::http::StatusCode;
use common::{add_link, insert_user, test_app};
use tiiny::{
    frontend::{Assets, BASE_URL_PLACEHOLDER},
    links::model::delete_link,
};

fn require_build() {
    assert!(
        Assets::get("index.html").is_some() && Assets::get("200.html").is_some(),
        "web/build is empty: run `bun run build` in web/ first"
    );
}

fn assert_html(r: &axum_test::TestResponse, p: &str) {
    r.assert_status_ok();
    assert!(
        r.header("content-type")
            .to_str()
            .unwrap()
            .starts_with("text/html"),
        "{p}"
    );
    assert_eq!(r.header("cache-control"), "no-cache", "{p}");
}

#[tokio::test(flavor = "multi_thread")]
async fn serves_pages_spa_and_assets() {
    require_build();
    let app = test_app(&[]).await;

    for p in [
        "/login",
        "/dashboard",
        "/dashboard/links/abc",
        "/admin/users",
        "/auth/verify?token=x",
    ] {
        let r = app.server.get(p).await;
        assert_html(&r, p);
        let body = r.text();
        assert!(body.contains("/_app/immutable"), "{p} is not the SPA shell");
        assert!(
            !body.contains("rel=\"canonical\""),
            "{p} is a prerendered page"
        );
    }

    let pages: Vec<String> = app
        .state
        .frontend
        .page_routes()
        .map(str::to_owned)
        .collect();
    for want in ["/", "/features", "/privacy", "/terms", "/report"] {
        assert!(
            pages.iter().any(|p| p == want),
            "{want} not derived from the build"
        );
    }
    for p in &pages {
        let canonical = format!("http://tiiny.test{p}");
        let r = app.server.get(p).await;
        assert_html(&r, p);
        let body = r.text();
        assert!(body.contains("<title>"), "{p}");
        assert!(
            body.contains(&format!("<link rel=\"canonical\" href=\"{canonical}\"")),
            "{p} canonical"
        );
    }

    let asset = Assets::iter()
        .find(|f| f.starts_with("_app/immutable/") && f.ends_with(".js"))
        .expect("an immutable js asset");
    let r = app.server.get(&format!("/{asset}")).await;
    r.assert_status_ok();
    assert_eq!(
        r.header("cache-control"),
        "public, max-age=31536000, immutable"
    );
    assert!(
        r.header("content-type")
            .to_str()
            .unwrap()
            .contains("javascript")
    );

    let r = app.server.get("/robots.txt").await;
    r.assert_status_ok();
    assert_eq!(r.header("cache-control"), "no-cache");
}

#[tokio::test(flavor = "multi_thread")]
async fn html_error_pages_and_api_404() {
    require_build();
    let app = test_app(&[]).await;

    for p in [
        "/nope123",
        "/_app/immutable/missing.js",
        "/a/b",
        "/../Cargo.toml",
        "/_app/../../Cargo.toml",
        "/404.html",
        "/410.html",
        "/200.html",
        "/index.html",
        "/features.html",
    ] {
        let r = app.server.get(p).await;
        r.assert_status(StatusCode::NOT_FOUND);
        assert!(
            r.header("content-type")
                .to_str()
                .unwrap()
                .starts_with("text/html"),
            "{p}"
        );
    }

    let uid = insert_user(&app.state.db, "a@x.com", "user").await;
    let l = add_link(&app, uid, "gone1", "https://example.com", true).await;
    delete_link(&app.state.db, l.id).await.unwrap();
    app.state.cache.evict("gone1");
    let r = app.server.get("/gone1").await;
    r.assert_status(StatusCode::GONE);
    assert!(r.text().contains("removed"), "410 page body");
    assert!(
        r.header("content-type")
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );

    for p in ["/api/v1/nope", "/api/v1/links/1/nope", "/api/v1"] {
        let r = app.server.get(p).await;
        r.assert_status(StatusCode::NOT_FOUND);
        let v: serde_json::Value = r.json();
        assert_eq!(v["error"]["code"], "not_found", "{p}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn security_headers_on_html_and_assets_not_redirects() {
    require_build();
    let app = test_app(&[]).await;
    for p in ["/", "/dashboard", "/nope123"] {
        let r = app.server.get(p).await;
        assert_eq!(r.header("x-frame-options"), "DENY", "{p}");
        assert_eq!(r.header("x-content-type-options"), "nosniff", "{p}");
        assert_eq!(
            r.header("referrer-policy"),
            "strict-origin-when-cross-origin",
            "{p}"
        );
    }
    let asset = Assets::iter()
        .find(|f| f.starts_with("_app/immutable/") && f.ends_with(".js"))
        .unwrap();
    let r = app.server.get(&format!("/{asset}")).await;
    assert_eq!(r.header("x-content-type-options"), "nosniff");
    assert!(r.maybe_header("x-frame-options").is_none());

    let uid = insert_user(&app.state.db, "a@x.com", "user").await;
    add_link(&app, uid, "hdr1", "https://example.com", false).await;
    let r = app.server.get("/hdr1").await;
    r.assert_status(StatusCode::FOUND);
    assert!(r.maybe_header("x-frame-options").is_none());
    assert!(r.maybe_header("referrer-policy").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn base_url_is_substituted_in_pages_sitemap_and_robots() {
    require_build();
    let app = test_app(&[("TIINY_BASE_URL", "https://short.example:8443/")]).await;
    let base = "https://short.example:8443";

    let home = app.server.get("/").await.text();
    for needle in [
        format!("<link rel=\"canonical\" href=\"{base}/\""),
        format!("<meta property=\"og:url\" content=\"{base}/\""),
        format!("<meta property=\"og:image\" content=\"{base}/og.png\""),
        format!("<meta name=\"twitter:image\" content=\"{base}/og.png\""),
        "\"@type\":\"SoftwareApplication\"".to_string(),
        "\"@type\":\"FAQPage\"".to_string(),
    ] {
        assert!(home.contains(&needle), "missing {needle}");
    }

    let shell = app.server.get("/dashboard").await.text();
    assert!(shell.contains(&format!("<meta name=\"tiiny-base-url\" content=\"{base}\"")));
    for f in Assets::iter().filter(|f| f.starts_with("_app/")) {
        let data = Assets::get(&f).unwrap().data;
        assert!(
            !data
                .windows(BASE_URL_PLACEHOLDER.len())
                .any(|w| w == BASE_URL_PLACEHOLDER.as_bytes()),
            "{f} is immutable-cached but holds the placeholder"
        );
    }

    let r = app.server.get("/sitemap.xml").await;
    r.assert_status_ok();
    assert!(r.header("content-type").to_str().unwrap().contains("xml"));
    let sitemap = r.text();
    let pages: Vec<&str> = app.state.frontend.page_routes().collect();
    assert_eq!(sitemap.matches("<loc>").count(), pages.len());
    for p in pages {
        assert!(sitemap.contains(&format!("<loc>{base}{p}</loc>")), "{p}");
    }
    assert!(!sitemap.contains("/dashboard") && !sitemap.contains("/login"));

    let robots = app.server.get("/robots.txt").await.text();
    assert!(robots.contains(&format!("Sitemap: {base}/sitemap.xml")));
    for d in [
        "/dashboard",
        "/admin",
        "/auth",
        "/api/",
        "/login",
        "/signup",
    ] {
        assert!(robots.contains(&format!("Disallow: {d}\n")), "{d}");
    }

    for p in [
        "/",
        "/features",
        "/privacy",
        "/terms",
        "/sitemap.xml",
        "/robots.txt",
        "/dashboard",
        "/nope123",
    ] {
        let body = app.server.get(p).await.text();
        assert!(
            !body.contains(BASE_URL_PLACEHOLDER),
            "{p} still has the placeholder"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn blocked_status_page_is_embedded() {
    require_build();
    let app = test_app(&[]).await;
    let r = app
        .state
        .frontend
        .status_page(StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS);
    assert_eq!(r.status(), StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS);
    assert!(
        r.headers()[axum::http::header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn relative_urls_and_no_sitemap_without_base_url() {
    require_build();
    let app = test_app(&[("TIINY_BASE_URL", "")]).await;
    let home = app.server.get("/").await.text();
    assert!(home.contains("<link rel=\"canonical\" href=\"/\""));
    assert!(!home.contains(BASE_URL_PLACEHOLDER));
    let robots = app.server.get("/robots.txt").await.text();
    assert!(robots.contains("Disallow: /admin\n") && !robots.contains("Sitemap"));
    assert_eq!(
        app.server.get("/sitemap.xml").await.status_code(),
        StatusCode::NOT_FOUND
    );
}
