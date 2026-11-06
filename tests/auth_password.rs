mod common;

use axum::http::StatusCode;
use common::{TestApp, cookie_app, json_server, raw_client, sql};
use serde_json::{Value, json};
use tiiny::{
    auth::{backend::promote_listed, session_store::LibsqlSessionStore},
    db::now_unix,
};

async fn signup(app: &TestApp, email: &str, password: &str) -> axum_test::TestResponse {
    app.server
        .post("/api/v1/auth/signup")
        .json(&json!({ "email": email, "password": password, "name": "Foo" }))
        .await
}

async fn login(app: &TestApp, email: &str, password: &str) -> axum_test::TestResponse {
    app.server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": email, "password": password }))
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn signup_sets_session_and_me_returns_user() {
    let app = cookie_app(&[]).await;
    let res = signup(&app, "a@x.com", "password1").await;
    assert_eq!(res.status_code(), StatusCode::CREATED);
    let set_cookie = res.header("set-cookie").to_str().unwrap().to_owned();
    assert!(set_cookie.starts_with("tiiny_session="), "{set_cookie}");
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    assert!(set_cookie.contains("SameSite=Lax"), "{set_cookie}");
    assert!(!set_cookie.contains("Secure"), "{set_cookie}");
    let me: Value = app.server.get("/api/v1/auth/me").await.json();
    assert_eq!(me["email"], "a@x.com");
    assert_eq!(me["email_verified"], false);
    assert_eq!(me["role"], "user");
    assert_eq!(me["name"], "Foo");
    assert_eq!(me["has_password"], true);
    assert!(me.get("password_hash").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn secure_cookie_for_https_base_url() {
    let app = cookie_app(&[("TIINY_BASE_URL", "https://tiiny.test")]).await;
    let res = signup(&app, "a@x.com", "password1").await;
    assert!(
        res.header("set-cookie")
            .to_str()
            .unwrap()
            .contains("Secure")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn email_casing_resolves_to_same_user() {
    let mut app = cookie_app(&[]).await;
    let res = signup(&app, " Foo@Example.COM ", "password1").await;
    assert_eq!(res.status_code(), StatusCode::CREATED);
    assert_eq!(res.json::<Value>()["email"], "foo@example.com");
    app.server.clear_cookies();
    assert_eq!(
        login(&app, "foo@example.com", "password1")
            .await
            .status_code(),
        StatusCode::OK
    );
    assert_eq!(
        signup(&app, "FOO@example.com", "password2")
            .await
            .status_code(),
        StatusCode::CONFLICT
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_credentials_are_generic_401() {
    let app = cookie_app(&[]).await;
    signup(&app, "a@x.com", "password1").await;
    for (email, pw) in [("a@x.com", "wrongpass"), ("nobody@x.com", "password1")] {
        let res = login(&app, email, pw).await;
        assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            res.json::<Value>(),
            json!({"error": {"code": "unauthorized", "message": "invalid email or password"}})
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn passwordless_account_cannot_password_login() {
    let app = cookie_app(&[]).await;
    common::insert_user(&app.state.db, "oauth@x.com", "user").await;
    let res = login(&app, "oauth@x.com", "password1").await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        res.json::<Value>()["error"]["message"],
        "invalid email or password"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn logout_ends_session() {
    let app = cookie_app(&[]).await;
    signup(&app, "a@x.com", "password1").await;
    assert_eq!(
        app.server.get("/api/v1/auth/me").await.status_code(),
        StatusCode::OK
    );
    let res = app.server.post("/api/v1/auth/logout").await;
    assert_eq!(res.status_code(), StatusCode::NO_CONTENT);
    assert_eq!(
        app.server.get("/api/v1/auth/me").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn signup_disabled_is_403() {
    let app = cookie_app(&[("TIINY_SIGNUP_ENABLED", "false")]).await;
    assert_eq!(
        signup(&app, "a@x.com", "password1").await.status_code(),
        StatusCode::FORBIDDEN
    );
    let p: Value = app.server.get("/api/v1/auth/providers").await.json();
    assert_eq!(p["signup"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn password_auth_disabled_is_404() {
    let app = cookie_app(&[("TIINY_PASSWORD_AUTH_ENABLED", "false")]).await;
    assert_eq!(
        signup(&app, "a@x.com", "password1").await.status_code(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        login(&app, "a@x.com", "password1").await.status_code(),
        StatusCode::NOT_FOUND
    );
    let p: Value = app.server.get("/api/v1/auth/providers").await.json();
    assert_eq!(
        p,
        json!({"password": false, "google": false, "github": false, "email_verification": false, "signup": true, "guest_links": true})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn password_length_is_validated() {
    let app = cookie_app(&[]).await;
    assert_eq!(
        signup(&app, "a@x.com", "short").await.status_code(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        signup(&app, "a@x.com", &"x".repeat(129))
            .await
            .status_code(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        signup(&app, "not-an-email", "password1")
            .await
            .status_code(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn disabled_user_cannot_login_and_session_dies() {
    let app = cookie_app(&[]).await;
    signup(&app, "a@x.com", "password1").await;
    sql(&app, "UPDATE users SET disabled_at = ?1", [now_unix()]).await;
    assert_eq!(
        app.server.get("/api/v1/auth/me").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
    let res = login(&app, "a@x.com", "password1").await;
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
    assert_eq!(
        res.json::<Value>()["error"]["message"],
        "This account has been disabled. Contact the administrator."
    );
    // A wrong password learns nothing about the account.
    let res = login(&app, "a@x.com", "wrong-password").await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        res.json::<Value>()["error"]["message"],
        "invalid email or password"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn mutations_require_json() {
    let app = cookie_app(&[]).await;
    let res = app
        .server
        .post("/api/v1/auth/login")
        .text(r#"{"email":"a@x.com","password":"password1"}"#)
        .content_type("text/plain")
        .await;
    assert_eq!(res.status_code(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(
        res.json::<Value>()["error"]["code"],
        "unsupported_media_type"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bodiless_mutations_require_json_content_type() {
    let app = cookie_app(&[]).await;
    let raw = raw_client(&app);
    for path in ["/api/v1/auth/logout", "/api/v1/auth/resend-verification"] {
        let res = raw.post(path).await;
        assert_eq!(
            res.status_code(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "{path}"
        );
    }
    assert_eq!(
        raw.delete("/api/v1/links/1").await.status_code(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    let res = raw
        .post("/api/v1/auth/logout")
        .content_type("application/json")
        .await;
    assert_ne!(res.status_code(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test(flavor = "multi_thread")]
async fn login_is_rate_limited_per_ip() {
    // A malformed email is rejected before argon2, so the attempts outpace the limiter's refill.
    let app = cookie_app(&[("TIINY_CLIENT_IP_HEADER", "x-real-ip")]).await;
    for _ in 0..10 {
        let res = app
            .server
            .post("/api/v1/auth/login")
            .add_header("x-real-ip", "1.2.3.4")
            .json(&json!({ "email": "not-an-email", "password": "password1" }))
            .await;
        assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
    }
    let res = app
        .server
        .post("/api/v1/auth/login")
        .add_header("x-real-ip", "1.2.3.4")
        .json(&json!({ "email": "not-an-email", "password": "password1" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res.json::<Value>()["error"]["code"], "too_many_requests");
    let other = app
        .server
        .post("/api/v1/auth/login")
        .add_header("x-real-ip", "5.6.7.8")
        .json(&json!({ "email": "not-an-email", "password": "password1" }))
        .await;
    assert_eq!(other.status_code(), StatusCode::UNAUTHORIZED);
    let logout = app
        .server
        .post("/api/v1/auth/logout")
        .add_header("x-real-ip", "1.2.3.4")
        .await;
    assert_eq!(logout.status_code(), StatusCode::NO_CONTENT);
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_email_promoted_at_signup() {
    let app = cookie_app(&[("TIINY_ADMIN_EMAILS", "Boss@x.com")]).await;
    let res = signup(&app, "boss@x.com", "password1").await;
    assert_eq!(res.json::<Value>()["role"], "admin");
    assert_eq!(
        signup(&app, "other@x.com", "password1")
            .await
            .json::<Value>()["role"],
        "user"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn existing_admin_emails_promoted_at_startup() {
    let app = cookie_app(&[("TIINY_ADMIN_EMAILS", "boss@x.com, second@x.com")]).await;
    let boss = common::insert_user(&app.state.db, "boss@x.com", "user").await;
    let second = common::insert_user(&app.state.db, "second@x.com", "user").await;
    let other = common::insert_user(&app.state.db, "other@x.com", "user").await;
    let conn = app.state.db.conn().await.unwrap();
    assert_eq!(
        promote_listed(conn.writer(), &app.state.config, None)
            .await
            .unwrap(),
        2
    );
    for (id, admin) in [(boss, 1), (second, 1), (other, 0)] {
        let q = "SELECT role = 'admin' FROM users WHERE id = ?1";
        assert_eq!(common::scalar(&app.state.db, q, [id]).await, admin);
    }
    assert_eq!(
        promote_listed(conn.writer(), &app.state.config, None)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sessions_persist_and_expired_are_purged() {
    let app = cookie_app(&[]).await;
    signup(&app, "a@x.com", "password1").await;
    let conn = app.state.db.conn().await.unwrap();
    let count = || async {
        let mut r = conn
            .query("SELECT COUNT(*) FROM sessions", ())
            .await
            .unwrap();
        r.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
    };
    assert_eq!(count().await, 1);
    let store = LibsqlSessionStore::new(app.state.db.clone());
    assert_eq!(store.purge_expired().await.unwrap(), 0);
    sql(
        &app,
        "UPDATE sessions SET expires_at = ?1",
        [now_unix() - 1],
    )
    .await;
    assert_eq!(
        app.server.get("/api/v1/auth/me").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(store.purge_expired().await.unwrap(), 1);
    assert_eq!(count().await, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn session_expiry_extended_at_most_daily() {
    let app = cookie_app(&[]).await;
    signup(&app, "a@x.com", "password1").await;
    let expires = || async {
        let conn = app.state.db.conn().await.unwrap();
        let mut r = conn
            .query("SELECT expires_at FROM sessions", ())
            .await
            .unwrap();
        r.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
    };
    let month = 30 * 86_400;
    let recent = now_unix() + month - 3600;
    sql(&app, "UPDATE sessions SET expires_at = ?1", [recent]).await;
    assert_eq!(
        app.server.get("/api/v1/auth/me").await.status_code(),
        StatusCode::OK
    );
    assert_eq!(expires().await, recent);
    let stale = now_unix() + month - 2 * 86_400;
    sql(&app, "UPDATE sessions SET expires_at = ?1", [stale]).await;
    assert_eq!(
        app.server.get("/api/v1/auth/me").await.status_code(),
        StatusCode::OK
    );
    assert!(expires().await >= now_unix() + month - 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_are_not_rate_limited() {
    let app = cookie_app(&[]).await;
    for _ in 0..15 {
        assert_eq!(
            app.server.get("/api/v1/auth/providers").await.status_code(),
            StatusCode::OK
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_email_promoted_again_at_login() {
    let mut app = cookie_app(&[("TIINY_ADMIN_EMAILS", "boss@x.com")]).await;
    signup(&app, "Boss@X.com", "password1").await;
    sql(&app, "UPDATE users SET role = ?1", ["user"]).await;
    app.server.clear_cookies();
    let res = login(&app, "boss@x.com", "password1").await;
    assert_eq!(res.json::<Value>()["role"], "admin");
}

fn session_cookie(res: &axum_test::TestResponse) -> String {
    res.cookie("tiiny_session").value().to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn session_id_rotates_on_every_login() {
    let mut app = cookie_app(&[]).await;
    signup(&app, "a@x.com", "password1").await;
    signup(&app, "b@x.com", "password1").await;
    app.server.clear_cookies();
    let anon = app.server.get("/api/v1/auth/providers").await;
    assert!(anon.maybe_cookie("tiiny_session").is_none());
    let a = session_cookie(&login(&app, "a@x.com", "password1").await);
    let b = session_cookie(&login(&app, "b@x.com", "password1").await);
    assert_ne!(a, b);
    let me: Value = app.server.get("/api/v1/auth/me").await.json();
    assert_eq!(me["email"], "b@x.com");
    let again = session_cookie(&login(&app, "b@x.com", "password1").await);
    assert_ne!(again, b);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_session_is_not_recreated() {
    use tower_sessions::{SessionStore, session::Id};
    let app = cookie_app(&[]).await;
    let res = signup(&app, "a@x.com", "password1").await;
    let id: Id = session_cookie(&res).parse().unwrap();
    let store = LibsqlSessionStore::new(app.state.db.clone());
    let mut record = store.load(&id).await.unwrap().unwrap();
    sql(&app, "DELETE FROM sessions", ()).await;
    record.expiry_date += tower_sessions::cookie::time::Duration::days(2);
    store.save(&record).await.unwrap();
    assert!(store.load(&id).await.unwrap().is_none());
    assert_eq!(
        app.server.get("/api/v1/auth/me").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn password_change_invalidates_sessions() {
    let app = cookie_app(&[]).await;
    signup(&app, "a@x.com", "password1").await;
    assert_eq!(
        app.server.get("/api/v1/auth/me").await.status_code(),
        StatusCode::OK
    );
    let hash = tiiny::auth::password::hash_password("password2").unwrap();
    sql(&app, "UPDATE users SET password_hash = ?1", [hash]).await;
    assert_eq!(
        app.server.get("/api/v1/auth/me").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn spoofed_leftmost_forwarded_for_does_not_escape_limit() {
    // Malformed email: no argon2, so timing can't refill the bucket mid-loop.
    let app = cookie_app(&[("TIINY_CLIENT_IP_HEADER", "x-forwarded-for")]).await;
    for i in 0..11 {
        let res = app
            .server
            .post("/api/v1/auth/login")
            .add_header("x-forwarded-for", format!("10.0.0.{i}, 1.2.3.4"))
            .json(&json!({ "email": "not-an-email", "password": "password1" }))
            .await;
        let want = if i < 10 {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::TOO_MANY_REQUESTS
        };
        assert_eq!(res.status_code(), want, "attempt {i}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_json_body_gets_json_error() {
    let app = cookie_app(&[]).await;
    let res = app
        .server
        .post("/api/v1/auth/login")
        .content_type("application/json")
        .bytes("{not json".into())
        .await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(res.json::<Value>()["error"]["code"], "bad_request");
}

#[tokio::test(flavor = "multi_thread")]
async fn session_of_disabled_user_still_rotates_on_next_login() {
    let mut app = cookie_app(&[]).await;
    signup(&app, "b@x.com", "password1").await;
    app.server.clear_cookies();
    let a = session_cookie(&signup(&app, "a@x.com", "password1").await);
    // `get_user` now returns None for this session, but its login data still carries an auth hash.
    sql(
        &app,
        "UPDATE users SET disabled_at = 1 WHERE email = 'a@x.com'",
        (),
    )
    .await;
    let b = session_cookie(&login(&app, "b@x.com", "password1").await);
    assert_ne!(a, b);
    let id: tower_sessions::session::Id = a.parse().unwrap();
    let store = LibsqlSessionStore::new(app.state.db.clone());
    use tower_sessions::SessionStore;
    assert!(
        store.load(&id).await.unwrap().is_none(),
        "old session row survived"
    );
}

fn plain(app: &TestApp) -> axum_test::TestServer {
    json_server(&app.state, false)
}

async fn guest_link(app: &TestApp, cookie: Option<&str>, url: &str) -> axum_test::TestResponse {
    let mut r = plain(app)
        .post("/api/v1/links")
        .json(&json!({ "url": url }));
    if let Some(c) = cookie {
        r = r.add_header("cookie", c);
    }
    let res = r.await;
    assert_eq!(res.status_code(), StatusCode::CREATED, "{}", res.text());
    res
}

async fn guest_listed(app: &TestApp, cookie: &str) -> Value {
    let res = plain(app)
        .get("/api/v1/links/guest")
        .add_header("cookie", cookie)
        .await;
    res.json::<Value>()["links"].clone()
}

async fn links_len(app: &TestApp) -> usize {
    let v: Value = app.server.get("/api/v1/links").await.json();
    v["links"].as_array().unwrap().len()
}

fn clears_guest(res: &axum_test::TestResponse) -> bool {
    res.headers().get_all("set-cookie").iter().any(|v| {
        let v = v.to_str().unwrap();
        v.starts_with("tiiny_guest=;") && v.contains("Max-Age=0")
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn signup_claims_guest_links() {
    let app = cookie_app(&[]).await;
    let c = common::guest_cookie(&guest_link(&app, None, "https://example.com/1").await);
    guest_link(&app, Some(&c), "https://example.com/2").await;
    let res = app
        .server
        .post("/api/v1/auth/signup")
        .add_header("cookie", c.as_str())
        .json(&json!({ "email": "a@x.com", "password": "password1", "name": "Foo" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::CREATED);
    assert!(clears_guest(&res));
    assert_eq!(links_len(&app).await, 2);
    assert_eq!(guest_listed(&app, &c).await, json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn login_claims_guest_links() {
    let app = cookie_app(&[]).await;
    signup(&app, "a@x.com", "password1").await;
    let c = common::guest_cookie(&guest_link(&app, None, "https://example.com/1").await);
    let res = app
        .server
        .post("/api/v1/auth/login")
        .add_header("cookie", c.as_str())
        .json(&json!({ "email": "a@x.com", "password": "password1" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);
    assert!(clears_guest(&res));
    assert_eq!(links_len(&app).await, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn claim_ignores_other_guests() {
    let app = cookie_app(&[]).await;
    signup(&app, "a@x.com", "password1").await;
    let mine = common::guest_cookie(&guest_link(&app, None, "https://example.com/1").await);
    let other = common::guest_cookie(&guest_link(&app, None, "https://example.com/2").await);
    app.server
        .post("/api/v1/auth/login")
        .add_header("cookie", mine.as_str())
        .json(&json!({ "email": "a@x.com", "password": "password1" }))
        .await;
    assert_eq!(links_len(&app).await, 1);
    assert_eq!(
        guest_listed(&app, &other).await.as_array().unwrap().len(),
        1
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn claim_ignores_daily_limit() {
    let app = cookie_app(&[("TIINY_MAX_LINKS_PER_USER_PER_DAY", "1")]).await;
    signup(&app, "a@x.com", "password1").await;
    app.server
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com/u" }))
        .await
        .assert_status(StatusCode::CREATED);
    let c = common::guest_cookie(&guest_link(&app, None, "https://example.com/1").await);
    guest_link(&app, Some(&c), "https://example.com/2").await;
    app.server
        .post("/api/v1/auth/login")
        .add_header("cookie", c.as_str())
        .json(&json!({ "email": "a@x.com", "password": "password1" }))
        .await;
    assert_eq!(links_len(&app).await, 3);
}
