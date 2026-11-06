mod common;

use axum::http::StatusCode;
use common::{signed_in, test_app};
use serde_json::{Value, json};

async fn make_link(s: &axum_test::TestServer, slug: &str, permanent: bool) -> String {
    let res = s
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com", "slug": slug }))
        .await;
    assert_eq!(res.status_code(), StatusCode::CREATED, "{}", res.text());
    if permanent {
        s.patch(&format!("/api/v1/links/{slug}"))
            .json(&json!({ "permanent": true }))
            .await
            .assert_status_ok();
    }
    res.json::<Value>()["slug"].as_str().unwrap().to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn non_admin_forbidden() {
    let app = test_app(&[]).await;
    let (s, _) = signed_in(&app, "a@x.com", "user").await;
    for p in [
        "/api/v1/admin/users",
        "/api/v1/admin/users/1",
        "/api/v1/admin/links",
    ] {
        assert_eq!(s.get(p).await.status_code(), StatusCode::FORBIDDEN);
    }
    assert_eq!(
        s.patch("/api/v1/admin/users/1")
            .json(&json!({ "role": "admin" }))
            .await
            .status_code(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        common::client(&app)
            .get("/api/v1/admin/users")
            .await
            .status_code(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_users_with_link_count_and_all_links() {
    let app = test_app(&[]).await;
    let (admin, _) = signed_in(&app, "admin@x.com", "admin").await;
    let (u, uid) = signed_in(&app, "u@x.com", "user").await;
    make_link(&u, "one", false).await;
    make_link(&u, "two", false).await;
    let res: Value = admin.get("/api/v1/admin/users").await.json();
    let users = res["users"].as_array().unwrap();
    assert_eq!(users.len(), 2);
    let me = users.iter().find(|x| x["id"] == uid).unwrap();
    assert_eq!(me["link_count"], 2);
    assert_eq!(me["email"], "u@x.com");
    assert_eq!(res["next_cursor"], Value::Null);
    let q: Value = admin.get("/api/v1/admin/users?query=u%40x").await.json();
    assert_eq!(q["users"].as_array().unwrap().len(), 1);
    let p: Value = admin.get("/api/v1/admin/users?limit=1").await.json();
    assert_eq!(p["users"].as_array().unwrap().len(), 1);
    assert!(p["next_cursor"].is_i64());
    let one: Value = admin
        .get(&format!("/api/v1/admin/users/{uid}"))
        .await
        .json();
    assert_eq!(
        (one["email"].as_str(), one["link_count"].as_i64()),
        (Some("u@x.com"), Some(2))
    );
    assert_eq!(
        admin.get("/api/v1/admin/users/999").await.status_code(),
        StatusCode::NOT_FOUND
    );

    make_link(&admin, "mine", false).await;
    let all: Value = admin.get("/api/v1/admin/links").await.json();
    assert_eq!(all["links"].as_array().unwrap().len(), 3);
    assert_eq!(all["links"][0]["short_url"], "http://tiiny.test/mine");
    let theirs: Value = admin
        .get(&format!("/api/v1/admin/links?user_id={uid}"))
        .await
        .json();
    assert_eq!(theirs["links"].as_array().unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn disable_user_revokes_sessions_and_links() {
    let app = test_app(&[]).await;
    let (admin, _) = signed_in(&app, "admin@x.com", "admin").await;
    let (u, uid) = signed_in(&app, "u@x.com", "user").await;
    make_link(&u, "theirs", false).await;
    assert_eq!(
        app.server.get("/theirs").await.status_code(),
        StatusCode::FOUND
    );
    let res = admin
        .patch(&format!("/api/v1/admin/users/{uid}"))
        .json(&json!({ "disabled": true }))
        .await;
    assert_eq!(res.status_code(), StatusCode::OK, "{}", res.text());
    assert_eq!(res.json::<Value>()["disabled"], true);
    assert_eq!(
        app.server.get("/theirs").await.status_code(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        u.get("/api/v1/auth/me").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        common::scalar(
            &app.state.db,
            "SELECT COUNT(*) FROM sessions WHERE user_id = ?1",
            [uid]
        )
        .await,
        0
    );
    admin
        .patch(&format!("/api/v1/admin/users/{uid}"))
        .json(&json!({ "disabled": false }))
        .await
        .assert_status_ok();
    assert_eq!(
        app.server.get("/theirs").await.status_code(),
        StatusCode::FOUND
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn mark_verified_and_change_role() {
    let app = test_app(&[]).await;
    let (admin, _) = signed_in(&app, "admin@x.com", "admin").await;
    let (_, uid) = signed_in(&app, "u@x.com", "user").await;
    let p = format!("/api/v1/admin/users/{uid}");
    let res: Value = admin
        .patch(&p)
        .json(&json!({ "email_verified": true }))
        .await
        .json();
    assert_eq!(res["email_verified"], true);
    let res: Value = admin
        .patch(&p)
        .json(&json!({ "role": "admin" }))
        .await
        .json();
    assert_eq!(res["role"], "admin");
    let res = admin.patch(&p).json(&json!({ "role": "owner" })).await;
    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res.json::<Value>()["error"]["code"], "unprocessable");
    assert_eq!(
        admin
            .patch("/api/v1/admin/users/9999")
            .json(&json!({ "disabled": true }))
            .await
            .status_code(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_user_cascades_and_retires_permanent() {
    let app = test_app(&[]).await;
    let (admin, _) = signed_in(&app, "admin@x.com", "admin").await;
    let (u, uid) = signed_in(&app, "u@x.com", "user").await;
    make_link(&u, "temp-link", false).await;
    make_link(&u, "perm-link", true).await;
    make_link(&u, "blk-link", false).await;
    common::sql(
        &app,
        "UPDATE links SET blocked_at = 1 WHERE slug = 'blk-link'",
        (),
    )
    .await;
    assert_eq!(
        app.server.get("/perm-link").await.status_code(),
        StatusCode::MOVED_PERMANENTLY
    );
    admin
        .delete(&format!("/api/v1/admin/users/{uid}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert_eq!(
        common::scalar(
            &app.state.db,
            "SELECT COUNT(*) FROM links WHERE user_id = ?1",
            [uid]
        )
        .await,
        0
    );
    assert_eq!(
        app.server.get("/perm-link").await.status_code(),
        StatusCode::GONE
    );
    assert_eq!(
        app.server.get("/blk-link").await.status_code(),
        StatusCode::GONE
    );
    assert_eq!(
        app.server.get("/temp-link").await.status_code(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        u.get("/api/v1/auth/me").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        admin
            .delete(&format!("/api/v1/admin/users/{uid}"))
            .await
            .status_code(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn self_protection() {
    let app = test_app(&[]).await;
    let (admin, aid) = signed_in(&app, "admin@x.com", "admin").await;
    let p = format!("/api/v1/admin/users/{aid}");
    for body in [json!({ "role": "user" }), json!({ "disabled": true })] {
        assert_eq!(
            admin.patch(&p).json(&body).await.status_code(),
            StatusCode::CONFLICT
        );
    }
    assert_eq!(admin.delete(&p).await.status_code(), StatusCode::CONFLICT);
    let res: Value = admin
        .patch(&p)
        .json(&json!({ "email_verified": true }))
        .await
        .json();
    assert_eq!(res["role"], "admin");
    assert_eq!(res["email_verified"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_edits_and_deletes_any_link() {
    let app = test_app(&[]).await;
    let (admin, _) = signed_in(&app, "admin@x.com", "admin").await;
    let (u, _) = signed_in(&app, "u@x.com", "user").await;
    let id = make_link(&u, "their-link", false).await;
    let p = format!("/api/v1/links/{id}");
    admin.get(&p).await.assert_status_ok();
    let res: Value = admin
        .patch(&p)
        .json(&json!({ "slug": "renamed" }))
        .await
        .json();
    assert_eq!(res["slug"], "renamed");
    assert_eq!(
        app.server.get("/renamed").await.status_code(),
        StatusCode::FOUND
    );
    assert_eq!(admin.get(&p).await.status_code(), StatusCode::NOT_FOUND);
    let p = "/api/v1/links/renamed";
    admin.get(&format!("{p}/stats")).await.assert_status_ok();
    admin.delete(p).await.assert_status(StatusCode::NO_CONTENT);
    assert_eq!(u.get(p).await.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_unblocks_link() {
    let app = test_app(&[]).await;
    let (admin, _) = signed_in(&app, "admin@x.com", "admin").await;
    let (u, _) = signed_in(&app, "u@x.com", "user").await;
    let id = make_link(&u, "their-link", false).await;
    let p = format!("/api/v1/admin/links/{id}");
    let res: Value = admin
        .patch(&p)
        .json(&json!({ "blocked": true }))
        .await
        .json();
    assert_eq!(res["blocked"], true);
    assert_eq!(app.server.get("/their-link").await.status_code(), 451);
    let res: Value = admin
        .patch(&p)
        .json(&json!({ "blocked": false }))
        .await
        .json();
    assert_eq!(res["blocked"], false);
    assert_eq!(
        app.server.get("/their-link").await.status_code(),
        StatusCode::FOUND
    );
    u.patch(&p)
        .json(&json!({ "blocked": true }))
        .await
        .assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_lists_retired_slugs() {
    let app = test_app(&[]).await;
    let (admin, _) = signed_in(&app, "admin@x.com", "admin").await;
    let (u, _) = signed_in(&app, "u@x.com", "user").await;
    for (slug, _) in [("first-one", 0), ("second-two", 1)] {
        let id = make_link(&u, slug, true).await;
        u.delete(&format!("/api/v1/links/{id}"))
            .json(&json!({}))
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    let v: Value = admin.get("/api/v1/admin/retired-slugs").await.json();
    let slugs: Vec<_> = v["slugs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, ["second-two", "first-one"]);
    assert!(v["slugs"][0]["id"].is_i64() && v["slugs"][0]["retired_at"].is_i64());
    let v: Value = admin
        .get("/api/v1/admin/retired-slugs?query=first")
        .await
        .json();
    assert_eq!(v["slugs"].as_array().unwrap().len(), 1);
    let v: Value = admin
        .get("/api/v1/admin/retired-slugs?limit=1")
        .await
        .json();
    assert_eq!(v["slugs"].as_array().unwrap().len(), 1);
    let cur = v["next_cursor"].as_i64().unwrap();
    let v: Value = admin
        .get(&format!("/api/v1/admin/retired-slugs?limit=1&cursor={cur}"))
        .await
        .json();
    assert_eq!(v["slugs"][0]["slug"], "first-one");
    assert!(v["next_cursor"].is_null());
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_releases_retired_slug() {
    let app = test_app(&[]).await;
    let (admin, _) = signed_in(&app, "admin@x.com", "admin").await;
    let (u, _) = signed_in(&app, "u@x.com", "user").await;
    let id = make_link(&u, "gone-slug", true).await;
    u.delete(&format!("/api/v1/links/{id}"))
        .json(&json!({}))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert_eq!(
        app.server.get("/gone-slug").await.status_code(),
        StatusCode::GONE
    );
    admin
        .delete("/api/v1/admin/retired-slugs/gone-slug")
        .json(&json!({}))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert_eq!(
        app.server.get("/gone-slug").await.status_code(),
        StatusCode::NOT_FOUND
    );
    make_link(&u, "gone-slug", false).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn release_unknown_slug_404() {
    let app = test_app(&[]).await;
    let (admin, _) = signed_in(&app, "admin@x.com", "admin").await;
    for slug in ["nope-slug", "a"] {
        assert_eq!(
            admin
                .delete(&format!("/api/v1/admin/retired-slugs/{slug}"))
                .json(&json!({}))
                .await
                .status_code(),
            StatusCode::NOT_FOUND
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn retired_slugs_non_admin_403() {
    let app = test_app(&[]).await;
    let (s, _) = signed_in(&app, "a@x.com", "user").await;
    assert_eq!(
        s.get("/api/v1/admin/retired-slugs").await.status_code(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        s.delete("/api/v1/admin/retired-slugs/abc")
            .json(&json!({}))
            .await
            .status_code(),
        StatusCode::FORBIDDEN
    );
}

async fn guest_link(app: &common::TestApp, url: &str) {
    common::client(app)
        .post("/api/v1/links")
        .json(&json!({ "url": url }))
        .await
        .assert_status(StatusCode::CREATED);
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_lists_guest_links() {
    let app = test_app(&[]).await;
    let (admin, _) = signed_in(&app, "admin@x.com", "admin").await;
    make_link(&admin, "mine", false).await;
    guest_link(&app, "https://one.example.com").await;
    guest_link(&app, "https://two.example.com").await;

    let res: Value = admin.get("/api/v1/admin/links?guest=true").await.json();
    let links = res["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    assert!(links.iter().all(|l| l["user_id"].is_null()));
    assert!(links.iter().all(|l| l.get("guest_key_hash").is_none()));
    assert!(links[0]["url"].as_str().unwrap().contains("two"));
    let users: Value = admin.get("/api/v1/admin/links?guest=false").await.json();
    assert_eq!(users["links"].as_array().unwrap().len(), 1);
    assert_eq!(users["links"][0]["slug"], "mine");
    let all: Value = admin.get("/api/v1/admin/links").await.json();
    assert_eq!(all["links"].as_array().unwrap().len(), 3);

    let q: Value = admin
        .get("/api/v1/admin/links?guest=true&query=one.example")
        .await
        .json();
    assert_eq!(q["links"].as_array().unwrap().len(), 1);

    let p: Value = admin
        .get("/api/v1/admin/links?guest=true&limit=1")
        .await
        .json();
    assert_eq!(p["links"].as_array().unwrap().len(), 1);
    let next = p["next_cursor"].as_i64().unwrap();
    let p2: Value = admin
        .get(&format!(
            "/api/v1/admin/links?guest=true&limit=1&cursor={next}"
        ))
        .await
        .json();
    assert!(p2["links"][0]["url"].as_str().unwrap().contains("one"));
    assert_eq!(p2["next_cursor"], Value::Null);
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_guest_and_user_id_422() {
    let app = test_app(&[]).await;
    let (admin, _) = signed_in(&app, "admin@x.com", "admin").await;
    let res = admin.get("/api/v1/admin/links?guest=true&user_id=1").await;
    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(res.text().contains("choose either guest or user_id"));
}

#[tokio::test(flavor = "multi_thread")]
async fn guest_filter_non_admin_403() {
    let app = test_app(&[]).await;
    let (s, _) = signed_in(&app, "a@x.com", "user").await;
    assert_eq!(
        s.get("/api/v1/admin/links?guest=true").await.status_code(),
        StatusCode::FORBIDDEN
    );
}
