mod common;

use axum::http::StatusCode;
use common::{client, signed_in, test_app};
use serde_json::{Value, json};

#[tokio::test(flavor = "multi_thread")]
async fn owner_round_trip() {
    let app = test_app(&[]).await;
    let (s, uid) = signed_in(&app, "a@x.com", "user").await;
    let res = s
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com", "slug": "abc" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::CREATED, "{}", res.text());
    let l: Value = res.json();
    assert_eq!(l["short_url"], "http://tiiny.test/abc");
    assert_eq!(l["user_id"], uid);
    assert_eq!(l["permanent"], false);
    let id = "abc";

    let got: Value = s.get(&format!("/api/v1/links/{id}")).await.json();
    assert_eq!(got["slug"], "abc");
    let list: Value = s.get("/api/v1/links").await.json();
    assert_eq!(list["links"].as_array().unwrap().len(), 1);
    assert_eq!(list["next_cursor"], Value::Null);

    let res = s
        .patch(&format!("/api/v1/links/{id}"))
        .json(&json!({ "url": "https://example.org" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::OK, "{}", res.text());
    assert_eq!(res.json::<Value>()["url"], "https://example.org/");

    let res = s.get(&format!("/api/v1/links/{id}/stats?range=7d")).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    assert_eq!(res.json::<Value>()["series"].as_array().unwrap().len(), 7);
    let res = s.get(&format!("/api/v1/links/{id}/stats?range=1y")).await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);

    let res = s.delete(&format!("/api/v1/links/{id}")).await;
    assert_eq!(res.status_code(), StatusCode::NO_CONTENT);
    assert_eq!(
        s.get(&format!("/api/v1/links/{id}")).await.status_code(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.server.get("/abc").await.status_code(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn other_user_gets_404() {
    let app = test_app(&[]).await;
    let (a, _) = signed_in(&app, "a@x.com", "user").await;
    let (b, _) = signed_in(&app, "b@x.com", "user").await;
    let id = a
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com" }))
        .await
        .json::<Value>()["slug"]
        .as_str()
        .unwrap()
        .to_owned();
    let p = format!("/api/v1/links/{id}");
    assert_eq!(b.get(&p).await.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(
        b.patch(&p)
            .json(&json!({ "url": "https://evil.com" }))
            .await
            .status_code(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(b.delete(&p).await.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(
        b.get(&format!("{p}/stats")).await.status_code(),
        StatusCode::NOT_FOUND
    );
    let list: Value = b.get("/api/v1/links").await.json();
    assert!(list["links"].as_array().unwrap().is_empty());
    assert_eq!(a.get(&p).await.status_code(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn unauthenticated_is_401() {
    let app = test_app(&[("TIINY_GUEST_LINKS_ENABLED", "false")]).await;
    let s = client(&app);
    assert_eq!(
        s.get("/api/v1/links").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        s.post("/api/v1/links")
            .json(&json!({ "url": "https://example.com" }))
            .await
            .status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        s.get("/api/v1/links/1").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn pagination_and_query() {
    let app = test_app(&[]).await;
    let (s, _) = signed_in(&app, "a@x.com", "user").await;
    for i in 0..5 {
        s.post("/api/v1/links")
            .json(&json!({ "url": format!("https://e{i}.com"), "slug": format!("slug{i}") }))
            .await
            .assert_status(StatusCode::CREATED);
    }
    let p1: Value = s.get("/api/v1/links?limit=2").await.json();
    let slugs = |v: &Value| -> Vec<String> {
        v["links"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["slug"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(slugs(&p1), ["slug4", "slug3"]);
    let c = p1["next_cursor"].as_i64().unwrap();
    let p2: Value = s
        .get(&format!("/api/v1/links?limit=2&cursor={c}"))
        .await
        .json();
    assert_eq!(slugs(&p2), ["slug2", "slug1"]);
    let c = p2["next_cursor"].as_i64().unwrap();
    let p3: Value = s
        .get(&format!("/api/v1/links?limit=2&cursor={c}"))
        .await
        .json();
    assert_eq!(slugs(&p3), ["slug0"]);
    assert_eq!(p3["next_cursor"], Value::Null);
    let exact: Value = s.get("/api/v1/links?limit=5").await.json();
    assert_eq!(exact["next_cursor"], Value::Null);
    let clamped: Value = s.get("/api/v1/links?limit=0").await.json();
    assert_eq!(slugs(&clamped).len(), 1);
    let q: Value = s.get("/api/v1/links?query=e3.com").await.json();
    assert_eq!(slugs(&q), ["slug3"]);
    let empty_q: Value = s.get("/api/v1/links?query=").await.json();
    assert_eq!(slugs(&empty_q).len(), 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn slug_change_takes_effect_immediately() {
    let app = test_app(&[]).await;
    let (s, _) = signed_in(&app, "a@x.com", "user").await;
    let id = s
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com", "slug": "old-slug" }))
        .await
        .json::<Value>()["slug"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        app.server.get("/old-slug").await.status_code(),
        StatusCode::FOUND
    );
    assert_eq!(
        app.server.get("/new-slug").await.status_code(),
        StatusCode::NOT_FOUND
    );
    s.patch(&format!("/api/v1/links/{id}"))
        .json(&json!({ "slug": "new-slug" }))
        .await
        .assert_status_ok();
    let res = app.server.get("/new-slug").await;
    assert_eq!(res.status_code(), StatusCode::FOUND);
    assert_eq!(res.header("location"), "https://example.com/");
    assert_eq!(
        app.server.get("/old-slug").await.status_code(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn create_evicts_cached_missing() {
    let app = test_app(&[]).await;
    let (s, _) = signed_in(&app, "a@x.com", "user").await;
    assert_eq!(
        app.server.get("/fresh").await.status_code(),
        StatusCode::NOT_FOUND
    );
    s.post("/api/v1/links")
        .json(&json!({ "url": "https://example.com", "slug": "fresh" }))
        .await
        .assert_status(StatusCode::CREATED);
    assert_eq!(
        app.server.get("/fresh").await.status_code(),
        StatusCode::FOUND
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn permanent_links() {
    let app = test_app(&[]).await;
    let (s, _) = signed_in(&app, "a@x.com", "user").await;
    let l: Value = s
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com", "slug": "perm", "permanent": true }))
        .await
        .json();
    assert_eq!(l["permanent"], true);
    let p = format!("/api/v1/links/{}", l["slug"].as_str().unwrap());
    assert_eq!(
        app.server.get("/perm").await.status_code(),
        StatusCode::MOVED_PERMANENTLY
    );
    let res = s.patch(&p).json(&json!({ "url": "https://x.com" })).await;
    assert_eq!(res.status_code(), StatusCode::CONFLICT);
    let res = s.patch(&p).json(&json!({ "permanent": false })).await;
    assert_eq!(res.status_code(), StatusCode::CONFLICT);
    s.delete(&p).await.assert_status(StatusCode::NO_CONTENT);
    assert_eq!(
        app.server.get("/perm").await.status_code(),
        StatusCode::GONE
    );

    let id = s
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com", "slug": "temp" }))
        .await
        .json::<Value>()["slug"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        app.server.get("/temp").await.status_code(),
        StatusCode::FOUND
    );
    let res = s
        .patch(&format!("/api/v1/links/{id}"))
        .json(&json!({ "permanent": true }))
        .await;
    assert_eq!(res.json::<Value>()["permanent"], true);
    assert_eq!(
        app.server.get("/temp").await.status_code(),
        StatusCode::MOVED_PERMANENTLY
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unverified_user_is_restricted() {
    let (app, _) = common::mail_app(&[]).await;
    let (s, uid) = signed_in(&app, "a@x.com", "user").await;
    let res = s
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::CREATED, "{}", res.text());
    let l: Value = res.json();
    assert_eq!(l["user_id"], uid);
    assert_eq!(l["permanent"], false);
    assert_eq!(l["slug"].as_str().unwrap().len(), 7);
    let res = s
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com", "slug": "mine" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
    assert_eq!(
        res.json::<Value>()["error"]["message"],
        "verify your email to choose a slug"
    );
    let res = s
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com", "permanent": true }))
        .await;
    assert_eq!(
        res.json::<Value>()["error"]["message"],
        "verify your email to make a link permanent"
    );
    let (admin, _) = signed_in(&app, "b@x.com", "admin").await;
    admin
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com", "slug": "adm" }))
        .await
        .assert_status(StatusCode::CREATED);
}

#[tokio::test(flavor = "multi_thread")]
async fn verified_user_unaffected() {
    let (app, _) = common::mail_app(&[]).await;
    let (s, uid) = signed_in(&app, "a@x.com", "user").await;
    common::sql(
        &app,
        "UPDATE users SET email_verified_at = 1 WHERE id = ?1",
        [uid],
    )
    .await;
    s.post("/api/v1/links")
        .json(&json!({ "url": "https://example.com", "slug": "verified", "permanent": true }))
        .await
        .assert_status(StatusCode::CREATED);
}

async fn guest_post(
    app: &common::TestApp,
    cookie: Option<&str>,
    body: Value,
) -> axum_test::TestResponse {
    let mut r = app.server.post("/api/v1/links").json(&body);
    if let Some(c) = cookie {
        r = r.add_header("cookie", c);
    }
    r.await
}

async fn guest_list(app: &common::TestApp, cookie: Option<&str>) -> Value {
    let mut r = app.server.get("/api/v1/links/guest");
    if let Some(c) = cookie {
        r = r.add_header("cookie", c);
    }
    r.await.json()
}

#[tokio::test(flavor = "multi_thread")]
async fn guest_creates_link_and_gets_cookie() {
    let app = test_app(&[]).await;
    let res = guest_post(&app, None, json!({ "url": "https://example.com" })).await;
    assert_eq!(res.status_code(), StatusCode::CREATED, "{}", res.text());
    let sc = res.header("set-cookie").to_str().unwrap().to_owned();
    assert!(sc.starts_with("tiiny_guest="), "{sc}");
    assert!(
        sc.contains("HttpOnly") && sc.contains("Path=/api/v1"),
        "{sc}"
    );
    assert!(
        sc.contains("SameSite=Lax") && sc.contains("Max-Age=34560000"),
        "{sc}"
    );
    assert!(!sc.contains("Secure"));
    let l: Value = res.json();
    assert_eq!(l["permanent"], false);
    assert_eq!(l["user_id"], Value::Null);
    assert_eq!(l["slug"].as_str().unwrap().len(), 7);
    let slug = l["slug"].as_str().unwrap();
    assert_eq!(
        app.server.get(&format!("/{slug}")).await.status_code(),
        StatusCode::FOUND
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn guest_cookie_secure_on_https() {
    let app = test_app(&[("TIINY_BASE_URL", "https://tiiny.test")]).await;
    let res = guest_post(&app, None, json!({ "url": "https://example.com" })).await;
    let sc = res.header("set-cookie").to_str().unwrap().to_owned();
    assert!(sc.starts_with("__Host-tiiny_guest="), "{sc}");
    assert!(sc.contains("Path=/;") || sc.ends_with("Path=/"), "{sc}");
    for attr in ["Secure", "HttpOnly", "SameSite=Lax", "Max-Age=34560000"] {
        assert!(sc.contains(attr), "{attr}: {sc}");
    }
    assert!(!sc.contains("Domain"), "{sc}");
}

#[tokio::test(flavor = "multi_thread")]
async fn plain_guest_cookie_ignored_on_https() {
    let app = test_app(&[("TIINY_BASE_URL", "https://tiiny.test")]).await;
    let secret = "A".repeat(43);
    let plain = format!("tiiny_guest={secret}");
    let res = guest_post(&app, Some(&plain), json!({ "url": "https://example.com" })).await;
    assert_eq!(res.status_code(), StatusCode::CREATED);
    assert!(
        res.header("set-cookie")
            .to_str()
            .unwrap()
            .starts_with("__Host-tiiny_guest=")
    );
    assert_eq!(guest_list(&app, Some(&plain)).await["links"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn guest_cookie_reused() {
    let app = test_app(&[]).await;
    let first = guest_post(&app, None, json!({ "url": "https://example.com/1" })).await;
    let c = common::guest_cookie(&first);
    let a = first.json::<Value>()["id"].as_i64().unwrap();
    let second = guest_post(&app, Some(&c), json!({ "url": "https://example.com/2" })).await;
    assert!(second.maybe_header("set-cookie").is_none());
    let b = second.json::<Value>()["id"].as_i64().unwrap();
    let list = guest_list(&app, Some(&c)).await;
    let ids: Vec<_> = list["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [b, a]);
}

#[tokio::test(flavor = "multi_thread")]
async fn guest_list_scoped_to_cookie() {
    let app = test_app(&[]).await;
    let c = common::guest_cookie(
        &guest_post(&app, None, json!({ "url": "https://example.com" })).await,
    );
    let other = common::guest_cookie(
        &guest_post(&app, None, json!({ "url": "https://example.org" })).await,
    );
    assert_ne!(c, other);
    assert_eq!(
        guest_list(&app, Some(&c)).await["links"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let none = guest_list(&app, None).await;
    assert_eq!(none, json!({ "links": [], "next_cursor": null }));
}

#[tokio::test(flavor = "multi_thread")]
async fn forged_guest_cookie_is_replaced() {
    let app = test_app(&[]).await;
    let res = guest_post(
        &app,
        Some("tiiny_guest=nope"),
        json!({ "url": "https://example.com" }),
    )
    .await;
    assert_eq!(res.status_code(), StatusCode::CREATED);
    assert!(
        res.header("set-cookie")
            .to_str()
            .unwrap()
            .starts_with("tiiny_guest=")
    );
    assert_ne!(common::guest_cookie(&res), "tiiny_guest=nope");
    assert_eq!(
        guest_list(&app, Some("tiiny_guest=nope")).await["links"],
        json!([])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn guest_custom_slug_and_permanent_forbidden() {
    let app = test_app(&[]).await;
    let res = guest_post(
        &app,
        None,
        json!({ "url": "https://example.com", "slug": "mine" }),
    )
    .await;
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
    assert_eq!(
        res.json::<Value>()["error"]["message"],
        "sign in to choose a slug"
    );
    let res = guest_post(
        &app,
        None,
        json!({ "url": "https://example.com", "permanent": true }),
    )
    .await;
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
    assert_eq!(
        res.json::<Value>()["error"]["message"],
        "sign in to make a link permanent"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn guest_daily_cap() {
    let app = test_app(&[("TIINY_MAX_GUEST_LINKS_PER_IP_PER_DAY", "2")]).await;
    let body = json!({ "url": "https://example.com" });
    let first = guest_post(&app, None, body.clone()).await;
    let c = common::guest_cookie(&first);
    guest_post(&app, Some(&c), body.clone())
        .await
        .assert_status(StatusCode::CREATED);
    let res = guest_post(&app, Some(&c), body).await;
    assert_eq!(res.status_code(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        res.json::<Value>()["error"]["message"],
        "you've reached today's link limit. Try again tomorrow"
    );
    let db = &app.state.db;
    assert_eq!(
        common::scalar(db, "SELECT count(*) FROM links", ()).await,
        2
    );
    assert_eq!(
        common::scalar(
            db,
            "SELECT created FROM link_quota WHERE owner LIKE 'ip:%'",
            ()
        )
        .await,
        2
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn guest_disabled() {
    let app = test_app(&[("TIINY_GUEST_LINKS_ENABLED", "false")]).await;
    let res = guest_post(&app, None, json!({ "url": "https://example.com" })).await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
    let p: Value = app.server.get("/api/v1/auth/providers").await.json();
    assert_eq!(p["guest_links"], false);
    let zero = test_app(&[("TIINY_MAX_GUEST_LINKS_PER_IP_PER_DAY", "0")]).await;
    assert_eq!(
        guest_post(&zero, None, json!({ "url": "https://example.com" }))
            .await
            .status_code(),
        StatusCode::UNAUTHORIZED
    );
    let on = test_app(&[]).await;
    assert_eq!(
        on.server
            .get("/api/v1/auth/providers")
            .await
            .json::<Value>()["guest_links"],
        true
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn guest_rate_limited_without_touching_login() {
    let app = test_app(&[]).await;
    let mut limited = 0;
    for _ in 0..11 {
        let res = guest_post(&app, None, json!({ "url": "https://example.com" })).await;
        if res.status_code() == StatusCode::TOO_MANY_REQUESTS {
            assert!(res.maybe_header("retry-after").is_some());
            limited += 1;
        }
    }
    assert_eq!(limited, 1);
    let res = app
        .server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": "a@x.com", "password": "password1" }))
        .await;
    assert_ne!(res.status_code(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_requests_get_json_errors() {
    let app = test_app(&[]).await;
    let (s, _) = signed_in(&app, "a@x.com", "user").await;
    let check = |res: axum_test::TestResponse, status: StatusCode, code: &str| {
        assert_eq!(res.status_code(), status, "{}", res.text());
        let v: Value = res.json();
        assert_eq!(v["error"]["code"], code);
        assert!(v["error"]["message"].is_string());
    };
    check(
        s.get("/api/v1/links?cursor=xyz").await,
        StatusCode::BAD_REQUEST,
        "bad_request",
    );
    check(
        s.get("/api/v1/links?limit=abc").await,
        StatusCode::BAD_REQUEST,
        "bad_request",
    );
    check(
        s.post("/api/v1/links")
            .json(&json!({ "slug": "nope" }))
            .await,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unprocessable",
    );
    check(
        s.post("/api/v1/links").content_type("text/plain").await,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "unsupported_media_type",
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn owner_cannot_edit_blocked_link() {
    let app = test_app(&[]).await;
    let (u, uid) = common::signed_in(&app, "u@x.com", "user").await;
    let l = common::add_link(&app, uid, "abc", "https://example.com", false).await;
    tiiny::links::model::set_blocked(&app.state.db, l.id, true)
        .await
        .unwrap();
    let res = u
        .patch(&format!("/api/v1/links/{}", l.slug))
        .json(&serde_json::json!({ "slug": "renamed" }))
        .await;
    res.assert_status(axum::http::StatusCode::FORBIDDEN);
    assert_eq!(
        res.json::<serde_json::Value>()["error"]["message"],
        "an admin blocked this link, so it can't be changed. You can still delete it"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_cannot_rename_blocked_link() {
    let app = test_app(&[]).await;
    let (_, uid) = common::signed_in(&app, "u@x.com", "user").await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let l = common::add_link(&app, uid, "abc", "https://example.com", false).await;
    tiiny::links::model::set_blocked(&app.state.db, l.id, true)
        .await
        .unwrap();
    let res = admin
        .patch("/api/v1/links/abc")
        .json(&json!({ "slug": "renamed" }))
        .await;
    res.assert_status(StatusCode::CONFLICT);
    assert_eq!(
        res.json::<serde_json::Value>()["error"]["message"],
        "slug can't change while the link is blocked"
    );
    // The destination can still change; the slug keeps answering 451.
    admin
        .patch("/api/v1/links/abc")
        .json(&json!({ "url": "https://example.org" }))
        .await
        .assert_status_ok();
    assert_eq!(
        app.server.get("/abc").await.status_code(),
        StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn owner_can_delete_blocked_link() {
    let app = test_app(&[]).await;
    let (u, uid) = common::signed_in(&app, "u@x.com", "user").await;
    let l = common::add_link(&app, uid, "abc", "https://example.com", false).await;
    tiiny::links::model::set_blocked(&app.state.db, l.id, true)
        .await
        .unwrap();
    u.delete(&format!("/api/v1/links/{}", l.slug))
        .await
        .assert_status(axum::http::StatusCode::NO_CONTENT);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_blocked_link_retires_slug() {
    let app = test_app(&[]).await;
    let (u, uid) = common::signed_in(&app, "u@x.com", "user").await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    for (by, slug) in [(&u, "own"), (&admin, "adm")] {
        let l = common::add_link(&app, uid, slug, "https://example.com", false).await;
        tiiny::links::model::set_blocked(&app.state.db, l.id, true)
            .await
            .unwrap();
        by.delete(&format!("/api/v1/links/{}", l.slug))
            .await
            .assert_status(StatusCode::NO_CONTENT);
        assert_eq!(
            app.server.get(&format!("/{slug}")).await.status_code(),
            StatusCode::GONE
        );
        let res = u
            .post("/api/v1/links")
            .json(&json!({ "url": "https://example.com", "slug": slug }))
            .await;
        res.assert_status(StatusCode::CONFLICT);
        assert_eq!(
            res.json::<Value>()["error"]["message"],
            "slug is already taken. Try another"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn unverified_user_cannot_edit_restricted_fields() {
    let (app, _) = common::mail_app(&[]).await;
    let (s, uid) = signed_in(&app, "a@x.com", "user").await;
    let l = common::add_link(&app, uid, "abc", "https://example.com", false).await;
    let path = format!("/api/v1/links/{}", l.slug);
    for (body, msg) in [
        (
            json!({ "slug": "mine" }),
            "verify your email to choose a slug",
        ),
        (
            json!({ "permanent": true }),
            "verify your email to make a link permanent",
        ),
    ] {
        let res = s.patch(&path).json(&body).await;
        res.assert_status(StatusCode::FORBIDDEN);
        assert_eq!(res.json::<Value>()["error"]["message"], msg);
    }
    s.patch(&path)
        .json(&json!({ "url": "https://example.org" }))
        .await
        .assert_status_ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn unverified_user_blocked_when_guest_links_disabled() {
    for vars in [
        [("TIINY_GUEST_LINKS_ENABLED", "false")],
        [("TIINY_MAX_GUEST_LINKS_PER_IP_PER_DAY", "0")],
    ] {
        let (app, _) = common::mail_app(&vars).await;
        let (s, _) = signed_in(&app, "a@x.com", "user").await;
        let res = s
            .post("/api/v1/links")
            .json(&json!({ "url": "https://example.com" }))
            .await;
        res.assert_status(StatusCode::FORBIDDEN);
        assert_eq!(
            res.json::<Value>()["error"]["message"],
            "verify your email to create links"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn guest_link_cannot_be_made_permanent() {
    let app = test_app(&[]).await;
    let (admin, _) = signed_in(&app, "admin@x.com", "admin").await;
    let res = guest_post(&app, None, json!({ "url": "https://example.com" })).await;
    let id = res.json::<Value>()["slug"].as_str().unwrap().to_owned();
    let res = admin
        .patch(&format!("/api/v1/links/{id}"))
        .json(&json!({ "permanent": true }))
        .await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res.json::<Value>()["error"]["message"],
        "guest links can't be permanent"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn link_to_request_host_rejected() {
    let app = test_app(&[("TIINY_BASE_URL", "")]).await;
    let (s, _) = signed_in(&app, "a@x.com", "user").await;
    let post = |url: &'static str| {
        s.post("/api/v1/links")
            .add_header("host", "Short.Example:8080")
            .json(&json!({ "url": url }))
    };
    let res = post("https://short.example/abc").await;
    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(res.text().contains("must not point at this service"));
    let res = post("https://example.com").await;
    assert_eq!(res.status_code(), StatusCode::CREATED, "{}", res.text());
    let l: Value = res.json();
    // No base URL: short links use the request's host.
    let slug = l["slug"].as_str().unwrap();
    let id = slug;
    assert_eq!(l["short_url"], format!("http://Short.Example:8080/{slug}"));
    let res = s
        .patch(&format!("/api/v1/links/{id}"))
        .add_header("host", "short.example")
        .json(&json!({ "url": "http://short.example./x" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test(flavor = "multi_thread")]
async fn guest_cookie_plain_without_base_url() {
    let app = test_app(&[("TIINY_BASE_URL", "")]).await;
    let res = guest_post(&app, None, json!({ "url": "https://example.com" })).await;
    let sc = res.header("set-cookie").to_str().unwrap().to_owned();
    assert!(
        sc.starts_with("tiiny_guest=") && !sc.contains("Secure"),
        "{sc}"
    );
}
