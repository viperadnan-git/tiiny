mod common;

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use common::{TestApp, mail_app, new_link, signup, sql, test_app};
use serde_json::{Value, json};
use tiiny::{
    auth::email::SentMail,
    db::now_unix,
    error::AppError,
    links::model::{Owner, UserRef, create_owned},
};

type Sent = Arc<Mutex<Vec<SentMail>>>;

fn token_in(sent: &Sent, path: &str) -> String {
    let marker = format!("http://tiiny.test{path}?token=");
    let mut body = None;
    for _ in 0..100 {
        // Mails send from spawned tasks, so arrival order isn't send order.
        body = (sent.lock().unwrap().iter().rev())
            .find(|m| m.body.contains(&marker))
            .map(|m| m.body.clone());
        if body.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let body = body.unwrap();
    let rest = body.split(&marker).nth(1).unwrap();
    rest.split(|c: char| c.is_whitespace())
        .next()
        .unwrap()
        .to_owned()
}

async fn gate(app: &TestApp, uid: i64) -> bool {
    let user = tiiny::auth::backend::find_user(&app.state.db, uid)
        .await
        .unwrap()
        .unwrap();
    app.state.mailer.is_enabled() && !user.email_verified && !user.is_admin()
}

async fn me(app: &TestApp) -> Value {
    app.server.get("/api/v1/auth/me").await.json()
}

async fn verify(app: &TestApp, token: &str) -> axum_test::TestResponse {
    app.server
        .post("/api/v1/auth/verify-email")
        .json(&json!({ "token": token }))
        .await
}

async fn wait_mails(sent: &Sent, n: usize) {
    for _ in 0..100 {
        if sent.lock().unwrap().len() >= n {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn signup_sends_verification_and_token_verifies_once() {
    let (app, sent) = mail_app(&[]).await;
    signup(&app.server, "a@x.com").await;
    wait_mails(&sent, 1).await;
    {
        let m = sent.lock().unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].to, "a@x.com");
        assert_eq!(m[0].subject, "Verify your email for tiiny");
    }
    let t = token_in(&sent, "/auth/verify");
    assert_eq!(me(&app).await["email_verified"], false);
    assert_eq!(verify(&app, &t).await.status_code(), StatusCode::NO_CONTENT);
    assert_eq!(me(&app).await["email_verified"], true);
    assert_eq!(verify(&app, &t).await.status_code(), StatusCode::CONFLICT);
    assert_eq!(
        verify(&app, "garbage").await.status_code(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn expired_token_rejected() {
    let (app, sent) = mail_app(&[]).await;
    signup(&app.server, "a@x.com").await;
    let t = token_in(&sent, "/auth/verify");
    sql(
        &app,
        "UPDATE email_tokens SET expires_at = ?1",
        [now_unix() - 1],
    )
    .await;
    assert_eq!(
        verify(&app, &t).await.status_code(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(me(&app).await["email_verified"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn token_ttls_and_hashed_storage() {
    let (app, sent) = mail_app(&[]).await;
    signup(&app.server, "a@x.com").await;
    let t = token_in(&sent, "/auth/verify");
    let conn = app.state.db.conn().await.unwrap();
    let mut rows = conn
        .query("SELECT token_hash, expires_at FROM email_tokens", ())
        .await
        .unwrap();
    let r = rows.next().await.unwrap().unwrap();
    let (h, exp): (String, i64) = (r.get(0).unwrap(), r.get(1).unwrap());
    assert_ne!(h, t);
    assert_eq!(h.len(), 64);
    assert!((exp - now_unix() - 86_400).abs() < 10);
}

#[tokio::test(flavor = "multi_thread")]
async fn resend_invalidates_previous_token() {
    let (app, sent) = mail_app(&[]).await;
    signup(&app.server, "a@x.com").await;
    let old = token_in(&sent, "/auth/verify");
    let res = app.server.post("/api/v1/auth/resend-verification").await;
    assert_eq!(res.status_code(), StatusCode::ACCEPTED);
    assert_eq!(sent.lock().unwrap().len(), 2);
    let new = token_in(&sent, "/auth/verify");
    assert_ne!(old, new);
    assert_eq!(
        verify(&app, &old).await.status_code(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        verify(&app, &new).await.status_code(),
        StatusCode::NO_CONTENT
    );
    let res = app.server.post("/api/v1/auth/resend-verification").await;
    assert_eq!(res.status_code(), StatusCode::CONFLICT);
}

#[tokio::test(flavor = "multi_thread")]
async fn resend_requires_login() {
    let (app, _) = mail_app(&[]).await;
    let res = app.server.post("/api/v1/auth/resend-verification").await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread")]
async fn unverified_user_cannot_choose_slug_until_verified() {
    let (app, sent) = mail_app(&[]).await;
    let uid = signup(&app.server, "a@x.com").await.json::<Value>()["id"]
        .as_i64()
        .unwrap();
    let (db, cfg) = (&app.state.db, &app.state.config);
    let u = UserRef {
        id: uid,
        is_admin: false,
    };
    let rv = gate(&app, uid).await;
    assert!(rv);
    let err = create_owned(
        db,
        &Owner::User(u.id),
        new_link(cfg, "https://example.com", Some("mine")),
        rv,
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(&err, AppError::Forbidden(m) if m == "verify your email to choose a slug"));
    verify(&app, &token_in(&sent, "/auth/verify")).await;
    let rv = gate(&app, uid).await;
    assert!(!rv);
    create_owned(
        db,
        &Owner::User(u.id),
        new_link(cfg, "https://example.com", None),
        rv,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn listed_admin_promoted_at_signup_before_verifying() {
    let (app, _sent) = mail_app(&[("TIINY_ADMIN_EMAILS", "boss@x.com")]).await;
    signup(&app.server, "boss@x.com").await;
    assert_eq!(me(&app).await["role"], "admin");
}

#[tokio::test(flavor = "multi_thread")]
async fn forgot_password_does_not_enumerate() {
    let (app, sent) = mail_app(&[]).await;
    signup(&app.server, "a@x.com").await;
    wait_mails(&sent, 1).await;
    sent.lock().unwrap().clear();
    let post = |e: &str| {
        app.server
            .post("/api/v1/auth/forgot-password")
            .json(&json!({ "email": e }))
    };
    let unknown = post("nobody@x.com").await;
    assert_eq!(unknown.status_code(), StatusCode::ACCEPTED);
    let known = post(" A@X.com ").await;
    assert_eq!(known.status_code(), StatusCode::ACCEPTED);
    assert_eq!(unknown.text(), known.text());
    wait_mails(&sent, 1).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let m = sent.lock().unwrap();
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].to, "a@x.com");
    assert_eq!(m[0].subject, "Reset your tiiny password");
    assert!(m[0].body.contains("http://tiiny.test/auth/reset?token="));
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_password_changes_password_and_logs_out_everywhere() {
    let (mut app, sent) = mail_app(&[]).await;
    signup(&app.server, "a@x.com").await;
    let login = |app: &TestApp, pw: &str| {
        app.server
            .post("/api/v1/auth/login")
            .json(&json!({ "email": "a@x.com", "password": pw }))
    };
    app.server.post("/api/v1/auth/logout").await;
    assert_eq!(login(&app, "password1").await.status_code(), StatusCode::OK);
    assert_eq!(me(&app).await["email"], "a@x.com");
    app.server
        .post("/api/v1/auth/forgot-password")
        .json(&json!({ "email": "a@x.com" }))
        .await;
    wait_mails(&sent, 2).await;
    let t = token_in(&sent, "/auth/reset");
    let reset = |pw: &str| {
        app.server
            .post("/api/v1/auth/reset-password")
            .json(&json!({ "token": &t, "password": pw }))
    };
    assert_eq!(
        reset("short").await.status_code(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        reset("newpassword1").await.status_code(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        reset("newpassword2").await.status_code(),
        StatusCode::BAD_REQUEST
    );
    let n = {
        let conn = app.state.db.conn().await.unwrap();
        let mut rows = conn
            .query("SELECT COUNT(*) FROM sessions", ())
            .await
            .unwrap();
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
    };
    assert_eq!(n, 0);
    assert_eq!(me(&app).await.get("email"), None);
    app.server.clear_cookies();
    assert_eq!(
        login(&app, "password1").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, "newpassword1").await.status_code(),
        StatusCode::OK
    );
    assert_eq!(me(&app).await["email_verified"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn tokens_are_purpose_bound() {
    let (app, sent) = mail_app(&[]).await;
    signup(&app.server, "a@x.com").await;
    let v = token_in(&sent, "/auth/verify");
    let res = app
        .server
        .post("/api/v1/auth/reset-password")
        .json(&json!({ "token": &v, "password": "newpassword1" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    app.server
        .post("/api/v1/auth/forgot-password")
        .json(&json!({ "email": "a@x.com" }))
        .await;
    let r = token_in(&sent, "/auth/reset");
    assert_eq!(
        verify(&app, &r).await.status_code(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(verify(&app, &v).await.status_code(), StatusCode::NO_CONTENT);
}

#[tokio::test(flavor = "multi_thread")]
async fn mail_endpoints_are_rate_limited() {
    let (app, _) = mail_app(&[]).await;
    let mut last = StatusCode::OK;
    for _ in 0..11 {
        last = app
            .server
            .post("/api/v1/auth/forgot-password")
            .json(&json!({ "email": "a@x.com" }))
            .await
            .status_code();
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test(flavor = "multi_thread")]
async fn mailer_disabled() {
    let app = test_app(&[]).await;
    for (path, body) in [
        ("forgot-password", json!({ "email": "a@x.com" })),
        (
            "reset-password",
            json!({ "token": "t", "password": "password1" }),
        ),
        ("verify-email", json!({ "token": "t" })),
    ] {
        let res = app
            .server
            .post(&format!("/api/v1/auth/{path}"))
            .json(&body)
            .await;
        assert_eq!(res.status_code(), StatusCode::NOT_FOUND, "{path}");
    }
    let p: Value = app.server.get("/api/v1/auth/providers").await.json();
    assert_eq!(p["email_verification"], false);
    let uid = common::insert_user(&app.state.db, "a@x.com", "user").await;
    let u = UserRef {
        id: uid,
        is_admin: false,
    };
    let rv = app.state.mailer.is_enabled();
    assert!(!rv);
    create_owned(
        &app.state.db,
        &Owner::User(u.id),
        new_link(&app.state.config, "https://example.com", None),
        rv,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn mailer_enabled_reports_provider() {
    let (app, _) = mail_app(&[]).await;
    let p: Value = app.server.get("/api/v1/auth/providers").await.json();
    assert_eq!(p["email_verification"], true);
}

async fn sessions_of(app: &TestApp, uid: i64) -> i64 {
    let conn = app.state.db.conn().await.unwrap();
    let mut rows = conn
        .query("SELECT COUNT(*) FROM sessions WHERE user_id = ?1", [uid])
        .await
        .unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn verify_requires_own_session_and_signs_out_others() {
    let (mut app, sent) = mail_app(&[]).await;
    let uid = signup(&app.server, "a@x.com").await.json::<Value>()["id"]
        .as_i64()
        .unwrap();
    let t = token_in(&sent, "/auth/verify");
    app.server.post("/api/v1/auth/logout").await;

    // No session: 401, token kept.
    app.server.clear_cookies();
    assert_eq!(
        verify(&app, &t).await.status_code(),
        StatusCode::UNAUTHORIZED
    );
    // Another user's session: 400, token kept.
    signup(&app.server, "b@x.com").await;
    assert_eq!(
        verify(&app, &t).await.status_code(),
        StatusCode::BAD_REQUEST
    );
    app.server.post("/api/v1/auth/logout").await;

    // Own session: ok, other sessions gone, current still works.
    app.server
        .post("/api/v1/auth/login")
        .json(&json!({"email":"a@x.com","password":"password1"}))
        .await;
    let first = app.server.get("/api/v1/auth/me").await;
    assert_eq!(first.status_code(), StatusCode::OK);
    sql(
        &app,
        "INSERT INTO sessions (id, payload, user_id, expires_at) VALUES ('other', x'7b7d', ?1, 9999999999)",
        [uid],
    )
    .await;
    assert_eq!(verify(&app, &t).await.status_code(), StatusCode::NO_CONTENT);
    assert_eq!(sessions_of(&app, uid).await, 1);
    assert_eq!(me(&app).await["email_verified"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_unlinks_oauth_and_sessions() {
    use tiiny::auth::oauth::{OauthIdentity, OauthProvider, resolve_oauth_user};
    let (app, sent) = mail_app(&[]).await;
    let uid = signup(&app.server, "victim@x.com").await.json::<Value>()["id"]
        .as_i64()
        .unwrap();
    let gh = OauthIdentity {
        provider: OauthProvider::Github,
        provider_user_id: "666".into(),
        email: "attacker@evil.com".into(),
        email_verified: true,
        name: None,
    };
    sql(
        &app,
        "INSERT INTO oauth_accounts (user_id, provider, provider_user_id, created_at) VALUES (?1, 'github', '666', 1)",
        [uid],
    )
    .await;
    app.server
        .post("/api/v1/auth/forgot-password")
        .json(&json!({ "email": "victim@x.com" }))
        .await;
    wait_mails(&sent, 2).await;
    let t = token_in(&sent, "/auth/reset");
    let res = app
        .server
        .post("/api/v1/auth/reset-password")
        .json(&json!({ "token": t, "password": "newpassword1" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::NO_CONTENT);
    assert_eq!(sessions_of(&app, uid).await, 0);
    let conn = app.state.db.conn().await.unwrap();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM oauth_accounts WHERE user_id = ?1",
            [uid],
        )
        .await
        .unwrap();
    assert_eq!(
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        0
    );
    drop(rows);
    let who = resolve_oauth_user(&app.state.db, &app.state.config, &gh)
        .await
        .unwrap();
    assert_ne!(who.id, uid);
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_reset_leaves_token_unspent() {
    let (app, sent) = mail_app(&[]).await;
    signup(&app.server, "a@x.com").await;
    app.server
        .post("/api/v1/auth/forgot-password")
        .json(&json!({ "email": "a@x.com" }))
        .await;
    wait_mails(&sent, 2).await;
    let t = token_in(&sent, "/auth/reset");
    // Fails the last write of the recovery, after the token and password updates.
    sql(
        &app,
        "CREATE TRIGGER fail_unlink BEFORE DELETE ON oauth_accounts BEGIN SELECT RAISE(ABORT, 'injected'); END",
        (),
    )
    .await;
    sql(
        &app,
        "INSERT INTO oauth_accounts (user_id, provider, provider_user_id, created_at) SELECT id, 'github', '1', 1 FROM users",
        (),
    )
    .await;
    let reset = || {
        app.server
            .post("/api/v1/auth/reset-password")
            .json(&json!({ "token": &t, "password": "newpassword1" }))
    };
    assert_eq!(
        reset().await.status_code(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let login = |pw: &str| {
        common::client(&app)
            .post("/api/v1/auth/login")
            .json(&json!({ "email": "a@x.com", "password": pw }))
    };
    assert_eq!(login("password1").await.status_code(), StatusCode::OK);
    sql(&app, "DROP TRIGGER fail_unlink", ()).await;
    assert_eq!(reset().await.status_code(), StatusCode::NO_CONTENT);
    assert_eq!(login("newpassword1").await.status_code(), StatusCode::OK);
}
