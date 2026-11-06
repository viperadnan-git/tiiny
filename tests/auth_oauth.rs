mod common;

use axum::http::StatusCode;
use common::{GOOGLE, TestApp, cookie_app, sql, start_state};
use serde_json::{Value, json};
use tiiny::{
    auth::oauth::{
        OauthError, OauthIdentity, OauthProvider, link_identity, resolve_oauth_user, unlink,
    },
    error::AppError,
};

fn ident(provider: OauthProvider, id: &str, email: &str, verified: bool) -> OauthIdentity {
    OauthIdentity {
        provider,
        provider_user_id: id.into(),
        email: email.into(),
        email_verified: verified,
        name: Some("Oauth Name".into()),
    }
}

async fn resolve(app: &TestApp, i: &OauthIdentity) -> Result<i64, OauthError> {
    resolve_oauth_user(&app.state.db, &app.state.config, i)
        .await
        .map(|u| u.id)
}

async fn scalar(app: &TestApp, q: &str, p: impl libsql::params::IntoParams) -> libsql::Value {
    common::first_row(&app.state.db, q, p)
        .await
        .get_value(0)
        .unwrap()
}

async fn link(app: &TestApp, uid: i64, i: &OauthIdentity) -> Result<bool, OauthError> {
    sql(
        app,
        "INSERT OR IGNORE INTO sessions (id, payload, user_id, expires_at) VALUES (?1, x'7b7d', ?2, 9999999999)",
        libsql::params![format!("s{uid}"), uid],
    )
    .await;
    link_identity(&app.state.db, uid, &format!("s{uid}"), i).await
}

async fn signup(app: &TestApp, email: &str) -> i64 {
    let res = common::signup(&app.server, email).await;
    assert_eq!(res.status_code(), StatusCode::CREATED);
    res.json::<Value>()["id"].as_i64().unwrap()
}

fn code(r: Result<impl std::fmt::Debug, OauthError>) -> &'static str {
    match r {
        Err(e) => e.code(),
        Ok(v) => panic!("expected error, got {v:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn new_identity_creates_verified_linked_user_and_is_stable() {
    let app = cookie_app(&[]).await;
    let i = ident(OauthProvider::Google, "g1", "new@x.com", true);
    let id = resolve(&app, &i).await.unwrap();
    assert_eq!(
        scalar(
            &app,
            "SELECT email_verified_at IS NOT NULL FROM users WHERE id = ?1",
            [id]
        )
        .await,
        libsql::Value::Integer(1)
    );
    assert_eq!(
        scalar(
            &app,
            "SELECT password_hash IS NULL FROM users WHERE id = ?1",
            [id]
        )
        .await,
        libsql::Value::Integer(1)
    );
    assert_eq!(
        scalar(
            &app,
            "SELECT provider FROM oauth_accounts WHERE user_id = ?1 AND provider_user_id = 'g1'",
            [id]
        )
        .await,
        libsql::Value::Text("google".into())
    );
    assert_eq!(resolve(&app, &i).await.unwrap(), id);
}

#[tokio::test(flavor = "multi_thread")]
async fn matching_verified_password_user_is_linked_and_keeps_password() {
    let app = cookie_app(&[]).await;
    let uid = signup(&app, "bob@x.com").await;
    sql(
        &app,
        "UPDATE users SET email_verified_at = 1 WHERE id = ?1",
        [uid],
    )
    .await;
    let i = ident(OauthProvider::Github, "77", "Bob@X.com", true);
    assert_eq!(resolve(&app, &i).await.unwrap(), uid);
    assert_eq!(
        scalar(
            &app,
            "SELECT password_hash IS NOT NULL FROM users WHERE id = ?1",
            [uid]
        )
        .await,
        libsql::Value::Integer(1)
    );
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM oauth_accounts WHERE user_id = ?1",
            [uid]
        )
        .await,
        libsql::Value::Integer(1)
    );
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM sessions WHERE user_id = ?1",
            [uid]
        )
        .await,
        libsql::Value::Integer(1)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn matching_unverified_password_user_is_claimed() {
    let app = cookie_app(&[]).await;
    let uid = signup(&app, "squat@x.com").await;
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM sessions WHERE user_id = ?1",
            [uid]
        )
        .await,
        libsql::Value::Integer(1)
    );
    let i = ident(OauthProvider::Google, "g2", "squat@x.com", true);
    assert_eq!(resolve(&app, &i).await.unwrap(), uid);
    assert_eq!(
        scalar(
            &app,
            "SELECT password_hash IS NULL AND email_verified_at IS NOT NULL FROM users WHERE id = ?1",
            [uid]
        )
        .await,
        libsql::Value::Integer(1)
    );
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM sessions WHERE user_id = ?1",
            [uid]
        )
        .await,
        libsql::Value::Integer(0)
    );
    assert_eq!(
        app.server.get("/api/v1/auth/me").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unverified_provider_email_is_rejected() {
    let app = cookie_app(&[]).await;
    let i = ident(OauthProvider::Github, "1", "x@x.com", false);
    assert_eq!(code(resolve(&app, &i).await), "email_unverified");
}

#[tokio::test(flavor = "multi_thread")]
async fn signup_disabled_blocks_new_but_links_existing() {
    let app = cookie_app(&[("TIINY_SIGNUP_ENABLED", "false")]).await;
    assert_eq!(
        code(resolve(&app, &ident(OauthProvider::Google, "g", "new@x.com", true)).await),
        "signup_disabled"
    );
    let uid = common::insert_user(&app.state.db, "old@x.com", "user").await;
    sql(
        &app,
        "UPDATE users SET email_verified_at = 1 WHERE id = ?1",
        [uid],
    )
    .await;
    let got = resolve(&app, &ident(OauthProvider::Google, "g", "old@x.com", true)).await;
    assert_eq!(got.unwrap(), uid);
}

#[tokio::test(flavor = "multi_thread")]
async fn disabled_user_is_rejected() {
    let app = cookie_app(&[]).await;
    let i = ident(OauthProvider::Google, "g", "d@x.com", true);
    let uid = resolve(&app, &i).await.unwrap();
    sql(
        &app,
        "UPDATE users SET disabled_at = 1 WHERE id = ?1",
        [uid],
    )
    .await;
    assert_eq!(code(resolve(&app, &i).await), "disabled");
    let other = ident(OauthProvider::Github, "9", "d@x.com", true);
    assert_eq!(code(resolve(&app, &other).await), "disabled");
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_email_is_promoted() {
    let app = cookie_app(&[("TIINY_ADMIN_EMAILS", "Boss@X.com")]).await;
    let uid = resolve(&app, &ident(OauthProvider::Github, "5", "BOSS@x.com", true))
        .await
        .unwrap();
    assert_eq!(
        scalar(&app, "SELECT email || role FROM users WHERE id = ?1", [uid]).await,
        libsql::Value::Text("boss@x.comadmin".into())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn link_mode_attaches_to_current_user_or_conflicts() {
    let app = cookie_app(&[]).await;
    let a = common::insert_user(&app.state.db, "a@x.com", "user").await;
    let b = common::insert_user(&app.state.db, "b@x.com", "user").await;
    let i = ident(OauthProvider::Github, "42", "someone-else@x.com", false);
    link(&app, a, &i).await.unwrap();
    link(&app, a, &i).await.unwrap();
    assert_eq!(code(link(&app, b, &i).await), "already_linked");
    assert_eq!(
        scalar(&app, "SELECT email FROM users WHERE id = ?1", [a]).await,
        libsql::Value::Text("a@x.com".into())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unlink_last_sign_in_method_conflicts() {
    let app = cookie_app(&[]).await;
    let uid = resolve(&app, &ident(OauthProvider::Google, "g", "o@x.com", true))
        .await
        .unwrap();
    resolve(&app, &ident(OauthProvider::Github, "h", "o@x.com", true))
        .await
        .unwrap();
    unlink(&app.state.db, &app.state.config, uid, OauthProvider::Github)
        .await
        .unwrap();
    assert!(matches!(
        unlink(&app.state.db, &app.state.config, uid, OauthProvider::Google).await,
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        unlink(&app.state.db, &app.state.config, uid, OauthProvider::Github).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn accounts_list_and_unlink_with_password() {
    let app = cookie_app(&[]).await;
    let uid = signup(&app, "p@x.com").await;
    sql(
        &app,
        "INSERT INTO oauth_accounts (user_id, provider, provider_user_id, created_at) VALUES (?1, 'github', '1', 0)",
        [uid],
    )
    .await;
    let list: Value = app.server.get("/api/v1/auth/accounts").await.json();
    assert_eq!(list, json!([{ "provider": "github" }]));
    let res = app.server.delete("/api/v1/auth/accounts/github").await;
    assert_eq!(res.status_code(), StatusCode::NO_CONTENT);
    let list: Value = app.server.get("/api/v1/auth/accounts").await.json();
    assert_eq!(list, json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn start_unconfigured_provider_is_404() {
    let app = cookie_app(&[]).await;
    let res = app.server.get("/api/v1/auth/oauth/google/start").await;
    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
    let res = app.server.get("/api/v1/auth/oauth/twitter/start").await;
    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn start_redirects_with_pkce_and_state() {
    let app = cookie_app(&GOOGLE).await;
    let res = app.server.get("/api/v1/auth/oauth/google/start").await;
    assert_eq!(res.status_code(), StatusCode::FOUND);
    let loc = url::Url::parse(res.header("location").to_str().unwrap()).unwrap();
    assert_eq!(loc.host_str(), Some("accounts.google.com"));
    let q: std::collections::HashMap<_, _> = loc.query_pairs().into_owned().collect();
    assert!(q.contains_key("code_challenge"));
    assert_eq!(q["code_challenge_method"], "S256");
    assert!(q.contains_key("state"));
    assert_eq!(q["client_id"], "gid");
    assert_eq!(
        q["redirect_uri"],
        "http://tiiny.test/api/v1/auth/oauth/google/callback"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn start_link_mode_requires_login() {
    let app = cookie_app(&GOOGLE).await;
    let res = app
        .server
        .get("/api/v1/auth/oauth/google/start?link=1")
        .await;
    assert_eq!(res.status_code(), StatusCode::FOUND);
    assert!(
        res.header("location")
            .to_str()
            .unwrap()
            .starts_with("/login")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn callback_rejects_missing_or_wrong_state_and_is_single_use() {
    let app = cookie_app(&GOOGLE).await;
    let res = app
        .server
        .get("/api/v1/auth/oauth/google/callback?code=c&state=s")
        .await;
    assert_eq!(res.status_code(), StatusCode::FOUND);
    assert_eq!(res.header("location"), "/login?error=state");

    let res = app.server.get("/api/v1/auth/oauth/google/start").await;
    let loc = url::Url::parse(res.header("location").to_str().unwrap()).unwrap();
    let state = loc
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    let res = app
        .server
        .get("/api/v1/auth/oauth/google/callback?code=c&state=wrong")
        .await;
    assert_eq!(res.header("location"), "/login?error=state");
    let res = app
        .server
        .get(&format!(
            "/api/v1/auth/oauth/google/callback?code=c&state={state}"
        ))
        .await;
    assert_eq!(res.header("location"), "/login?error=state");
}

#[tokio::test(flavor = "multi_thread")]
async fn change_password_keeps_current_session_and_revokes_others() {
    let mut app = cookie_app(&[]).await;
    let uid = signup(&app, "c@x.com").await;
    let pre_cycle = app
        .server
        .post("/api/v1/auth/login")
        .json(&json!({ "email": "c@x.com", "password": "password1" }))
        .await
        .cookie("tiiny_session");
    let res = app
        .server
        .put("/api/v1/auth/password")
        .json(&json!({ "new_password": "password2" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    let res = app
        .server
        .put("/api/v1/auth/password")
        .json(&json!({ "current_password": "nope-nope", "new_password": "password2" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
    let res = app
        .server
        .put("/api/v1/auth/password")
        .json(&json!({ "current_password": "password1", "new_password": "short" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    let res = app
        .server
        .put("/api/v1/auth/password")
        .json(&json!({ "current_password": "password1", "new_password": "password2" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::NO_CONTENT);
    assert_eq!(
        app.server.get("/api/v1/auth/me").await.status_code(),
        StatusCode::OK
    );
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM sessions WHERE user_id = ?1",
            [uid]
        )
        .await,
        libsql::Value::Integer(1)
    );
    app.server.clear_cookies();
    let res = app
        .server
        .get("/api/v1/auth/me")
        .add_cookie(pre_cycle)
        .await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread")]
async fn patch_me_name() {
    let app = cookie_app(&[]).await;
    signup(&app, "n@x.com").await;
    let res = app
        .server
        .patch("/api/v1/auth/me")
        .json(&json!({ "name": "  New Name  " }))
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);
    assert_eq!(res.json::<Value>()["name"], "New Name");
    let res = app
        .server
        .patch("/api/v1/auth/me")
        .json(&json!({ "name": "x".repeat(101) }))
        .await;
    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    let res = app
        .server
        .patch("/api/v1/auth/me")
        .json(&json!({ "name": "   " }))
        .await;
    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_me_retires_permanent_slugs_and_blocks_last_admin() {
    let app = cookie_app(&[]).await;
    let uid = signup(&app, "del@x.com").await;
    for (slug, perm) in [("perm1", "1"), ("temp1", "NULL")] {
        sql(
            &app,
            &format!(
                "INSERT INTO links (user_id, slug, url, permanent_at, created_at, updated_at)
                 VALUES (?1, ?2, 'https://e.com', {perm}, 0, 0)"
            ),
            libsql::params![uid, slug],
        )
        .await;
    }
    sql(&app, "UPDATE users SET role = 'admin' WHERE id = ?1", [uid]).await;
    let res = app.server.delete("/api/v1/auth/me").await;
    assert_eq!(res.status_code(), StatusCode::CONFLICT);

    common::insert_user(&app.state.db, "admin2@x.com", "admin").await;
    let res = app.server.delete("/api/v1/auth/me").await;
    assert_eq!(res.status_code(), StatusCode::NO_CONTENT);
    assert_eq!(
        scalar(&app, "SELECT count(*) FROM users WHERE id = ?1", [uid]).await,
        libsql::Value::Integer(0)
    );
    assert_eq!(
        scalar(&app, "SELECT group_concat(slug) FROM retired_slugs", ()).await,
        libsql::Value::Text("perm1".into())
    );
    assert_eq!(
        scalar(
            &app,
            "SELECT count(*) FROM sessions WHERE user_id = ?1",
            [uid]
        )
        .await,
        libsql::Value::Integer(0)
    );
    assert_eq!(
        app.server.get("/api/v1/auth/me").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn claim_drops_squatter_links() {
    let app = cookie_app(&[]).await;
    let victim = signup(&app, "victim@x.com").await;
    let gh = ident(OauthProvider::Github, "666", "attacker@evil.com", true);
    link(&app, victim, &gh).await.unwrap();
    let google = ident(OauthProvider::Google, "g-victim", "victim@x.com", true);
    assert_eq!(resolve(&app, &google).await.unwrap(), victim);
    let list: Vec<String> = tiiny::auth::oauth::list_providers(&app.state.db, victim)
        .await
        .unwrap();
    assert_eq!(list, ["google"]);
    let attacker = resolve(&app, &gh).await.unwrap();
    assert_ne!(attacker, victim);
    let mut unverified = ident(OauthProvider::Github, "667", "victim@x.com", false);
    assert_eq!(code(resolve(&app, &unverified).await), "email_unverified");
    unverified.email_verified = true;
    // Victim is verified now and has no GitHub link, so a verified identity links normally.
    assert_eq!(resolve(&app, &unverified).await.unwrap(), victim);
}

#[tokio::test(flavor = "multi_thread")]
async fn one_identity_per_provider_per_user() {
    let app = cookie_app(&[]).await;
    let uid = resolve(&app, &ident(OauthProvider::Google, "g1", "one@x.com", true))
        .await
        .unwrap();
    let second = ident(OauthProvider::Google, "g2", "one@x.com", true);
    assert_eq!(code(link(&app, uid, &second).await), "already_linked");
    assert_eq!(code(resolve(&app, &second).await), "already_linked");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_first_callbacks_resolve_to_one_user() {
    let app = cookie_app(&[]).await;
    for n in 0..20 {
        let tasks: Vec<_> = (0..8)
            .map(|_| {
                let (db, config) = (app.state.db.clone(), app.state.config.clone());
                tokio::spawn(async move {
                    let i = ident(
                        OauthProvider::Github,
                        &format!("c{n}"),
                        &format!("c{n}@x.com"),
                        true,
                    );
                    resolve_oauth_user(&db, &config, &i)
                        .await
                        .map(|u| u.id)
                        .map_err(|e| format!("{e:?}"))
                })
            })
            .collect();
        let mut ids = vec![];
        for t in tasks {
            ids.push(t.await.unwrap().unwrap());
        }
        ids.dedup();
        assert_eq!(ids.len(), 1, "{ids:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn start_while_logged_in_goes_to_dashboard() {
    let app = cookie_app(&GOOGLE).await;
    signup(&app, "in@x.com").await;
    let res = app.server.get("/api/v1/auth/oauth/google/start").await;
    assert_eq!(res.status_code(), StatusCode::FOUND);
    assert_eq!(res.header("location"), "/dashboard");
}

#[tokio::test(flavor = "multi_thread")]
async fn link_mode_requires_verified_email() {
    let app = cookie_app(&GOOGLE).await;
    let uid = signup(&app, "lv@x.com").await;
    let res = app
        .server
        .get("/api/v1/auth/oauth/google/start?link=1")
        .await;
    assert_eq!(
        res.header("location"),
        "/dashboard/settings?error=verify_email_first"
    );

    sql(
        &app,
        "UPDATE users SET email_verified_at = 1 WHERE id = ?1",
        [uid],
    )
    .await;
    let res = app
        .server
        .get("/api/v1/auth/oauth/google/start?link=1")
        .await;
    let loc = url::Url::parse(res.header("location").to_str().unwrap()).unwrap();
    let state = loc
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    sql(
        &app,
        "UPDATE users SET email_verified_at = NULL WHERE id = ?1",
        [uid],
    )
    .await;
    let res = app
        .server
        .get(&format!(
            "/api/v1/auth/oauth/google/callback?code=c&state={state}"
        ))
        .await;
    assert_eq!(
        res.header("location"),
        "/dashboard/settings?error=verify_email_first"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn link_requires_initiating_session_to_still_exist() {
    let app = cookie_app(&[]).await;
    let uid = common::insert_user(&app.state.db, "v@x.com", "user").await;
    let i = ident(OauthProvider::Github, "9", "x@y.com", true);
    assert!(link(&app, uid, &i).await.is_ok());
    sql(&app, "DELETE FROM oauth_accounts", ()).await;
    sql(&app, "DELETE FROM sessions WHERE user_id = ?1", [uid]).await;
    assert!(
        !link_identity(&app.state.db, uid, &format!("s{uid}"), &i)
            .await
            .unwrap()
    );
    assert_eq!(
        scalar(&app, "SELECT COUNT(*) FROM oauth_accounts", ()).await,
        libsql::Value::Integer(0)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn callback_provider_error_redirects_denied_and_clears_state() {
    let app = cookie_app(&GOOGLE).await;
    let cb = "/api/v1/auth/oauth/google/callback";
    let res = app
        .server
        .get(&format!("{cb}?error=access_denied&state=x"))
        .await;
    assert_eq!(res.status_code(), StatusCode::FOUND);
    assert_eq!(res.header("location"), "/login?error=denied");

    let state = start_state(&app).await;
    let res = app
        .server
        .get(&format!("{cb}?error=access_denied&state={state}"))
        .await;
    assert_eq!(res.header("location"), "/login?error=denied");
    let res = app.server.get(&format!("{cb}?code=c&state={state}")).await;
    assert_eq!(res.header("location"), "/login?error=state");
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth_routes_redirect_on_bad_query() {
    let app = cookie_app(&GOOGLE).await;
    let cb = "/api/v1/auth/oauth/google/callback";
    for q in ["", "?state=a&state=b", "?code=c"] {
        let res = app.server.get(&format!("{cb}{q}")).await;
        assert_eq!(res.status_code(), StatusCode::FOUND, "{q}");
        assert_eq!(res.header("location"), "/login?error=state", "{q}");
    }
    let res = app
        .server
        .get("/api/v1/auth/oauth/google/start?link=1&link=2")
        .await;
    assert_eq!(res.status_code(), StatusCode::FOUND);
    assert_eq!(res.header("location"), "/login?error=state");
}

#[tokio::test(flavor = "multi_thread")]
async fn start_carries_only_safe_next() {
    let app = cookie_app(&GOOGLE).await;
    signup(&app, "n@x.com").await;
    for (next, want) in [
        (
            "/dashboard%3Furl%3Dhttps%253A%252F%252Fa.b",
            "/dashboard?url=https%3A%2F%2Fa.b",
        ),
        ("//evil.com", "/dashboard"),
        ("/%5Cevil.com", "/dashboard"),
        ("https://evil.com", "/dashboard"),
    ] {
        let res = app
            .server
            .get(&format!("/api/v1/auth/oauth/google/start?next={next}"))
            .await;
        assert_eq!(res.header("location"), want, "{next}");
    }
}
