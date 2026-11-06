mod common;

use axum::http::StatusCode;
use common::{
    GOOGLE, TestApp, client, first_row, insert_user, mail_app, sha256_hex, signed_in, start_state,
    test_app,
};
use serde_json::{Value, json};
use tiiny::{
    auth::oauth::{OauthIdentity, OauthProvider, resolve_oauth_user},
    db::now_unix,
};

/// A fresh router per request, so the auth rate limit never interferes.
async fn post(app: &TestApp, path: &str, body: Value) -> axum_test::TestResponse {
    client(app).post(path).json(&body).await
}

fn assert_err(res: &axum_test::TestResponse, status: StatusCode, ctx: &str) {
    assert_eq!(res.status_code(), status, "{ctx}: {}", res.text());
    let v: Value = res.json();
    assert!(v["error"]["code"].is_string(), "{ctx}: {v}");
    assert!(v["error"]["message"].is_string(), "{ctx}: {v}");
}

async fn signup(app: &TestApp, email: &str, name: Value) -> axum_test::TestResponse {
    post(
        app,
        "/api/v1/auth/signup",
        json!({ "email": email, "password": "password1", "name": name }),
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn signup_accepts_unicode_names_and_normalises() {
    let app = test_app(&[]).await;
    for (i, (raw, want)) in [
        ("José", "José"),
        ("Zoë", "Zoë"),
        ("Ngô Bảo Châu", "Ngô Bảo Châu"),
        ("O'Brien", "O'Brien"),
        ("O’Brien", "O’Brien"),
        ("D’Angelo", "D’Angelo"),
        ("Jean-Luc", "Jean-Luc"),
        ("A. Smith", "A. Smith"),
        ("  Ada \t  Lovelace  ", "Ada Lovelace"),
        ("Ada\u{85}Lovelace", "Ada Lovelace"),
        (&"a".repeat(100), &"a".repeat(100)),
    ]
    .into_iter()
    .enumerate()
    {
        let res = signup(&app, &format!("u{i}@x.com"), json!(raw)).await;
        assert_eq!(
            res.status_code(),
            StatusCode::CREATED,
            "{raw}: {}",
            res.text()
        );
        assert_eq!(res.json::<Value>()["name"], want, "{raw}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn signup_rejects_bad_or_missing_names() {
    let app = test_app(&[]).await;
    for name in [
        json!("dev123"),
        json!("😀"),
        json!(""),
        json!(" "),
        json!("-Bob"),
        json!("a".repeat(101)),
        json!("Ada\u{FEFF}"),
    ] {
        let res = signup(&app, "n@x.com", name.clone()).await;
        assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, &name.to_string());
    }
    for name in [Value::Null, json!(""), json!(" \u{85} ")] {
        let res = signup(&app, "n@x.com", name.clone()).await;
        assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, &name.to_string());
        assert_eq!(res.json::<Value>()["error"]["message"], "name is required");
    }
    let res = post(
        &app,
        "/api/v1/auth/signup",
        json!({ "email": "n@x.com", "password": "password1" }),
    )
    .await;
    assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, "missing name");
    assert_eq!(res.json::<Value>()["error"]["message"], "name is required");
}

#[tokio::test(flavor = "multi_thread")]
async fn patch_me_name_is_required_and_validated() {
    let app = test_app(&[]).await;
    let (s, _) = signed_in(&app, "m@x.com", "user").await;
    let res = s
        .patch("/api/v1/auth/me")
        .json(&json!({ "name": "dev123" }))
        .await;
    assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, "dev123");
    for body in [
        json!({ "name": "" }),
        json!({ "name": "   " }),
        json!({ "name": null }),
        json!({}),
    ] {
        let res = s.patch("/api/v1/auth/me").json(&body).await;
        assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, &body.to_string());
        assert_eq!(res.json::<Value>()["error"]["message"], "name is required");
    }
    let res = s
        .patch("/api/v1/auth/me")
        .json(&json!({ "name": " Zoë   Smith " }))
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);
    assert_eq!(res.json::<Value>()["name"], "Zoë Smith");
}

#[tokio::test(flavor = "multi_thread")]
async fn signup_rejects_invalid_emails() {
    let app = test_app(&[]).await;
    let long_local = format!("{}@x.com", "a".repeat(65));
    let long = format!("a@{}.com", "b".repeat(250));
    for email in [
        "plain",
        "a@b",
        "a@@b.com",
        "a@b@c.com",
        "@b.com",
        "a b@c.com",
        "a@-b.com",
        "a@b-.com",
        "a@b..com",
        "a@b_c.com",
        "a@bü.com",
        long_local.as_str(),
        long.as_str(),
    ] {
        let res = signup(&app, email, json!("Ann")).await;
        assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, email);
    }
    for (i, email) in ["  Foo@Example.COM ", "a@xn--bcher-kva.example"]
        .iter()
        .enumerate()
    {
        let res = signup(&app, email, json!(format!("Ann {}", "B".repeat(i + 1)))).await;
        assert_eq!(res.status_code(), StatusCode::CREATED, "{email}");
    }
    let res = signup(&app, "foo@example.com", json!("Ann")).await;
    assert_eq!(res.status_code(), StatusCode::CONFLICT);
    let local64 = format!("{}@x.com", "a".repeat(64));
    let res = signup(&app, &local64, json!("Ann")).await;
    assert_eq!(res.status_code(), StatusCode::CREATED);
    // Bytes, not chars: 32 × "é" is 64 bytes, 33 is 66.
    let res = signup(&app, &format!("{}@x.com", "é".repeat(32)), json!("Ann")).await;
    assert_eq!(res.status_code(), StatusCode::CREATED);
    let res = signup(&app, &format!("{}@x.com", "é".repeat(33)), json!("Ann")).await;
    assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, "66-byte local part");
}

#[tokio::test(flavor = "multi_thread")]
async fn login_with_invalid_email_is_generic_401() {
    let app = test_app(&[]).await;
    let res = post(
        &app,
        "/api/v1/auth/login",
        json!({ "email": "not an email", "password": "password1" }),
    )
    .await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        res.json::<Value>()["error"]["message"],
        "invalid email or password"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn forgot_password_rejects_invalid_email() {
    let (app, _) = mail_app(&[]).await;
    let res = post(
        &app,
        "/api/v1/auth/forgot-password",
        json!({ "email": "a@b" }),
    )
    .await;
    assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, "forgot");
    let res = post(
        &app,
        "/api/v1/auth/forgot-password",
        json!({ "email": "nobody@x.com" }),
    )
    .await;
    assert_eq!(res.status_code(), StatusCode::ACCEPTED);
}

#[tokio::test(flavor = "multi_thread")]
async fn whitespace_only_passwords_rejected() {
    let app = test_app(&[]).await;
    let res = post(
        &app,
        "/api/v1/auth/signup",
        json!({ "email": "w@x.com", "password": "         ", "name": "Ann" }),
    )
    .await;
    assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, "signup");
    let (s, _) = signed_in(&app, "p@x.com", "user").await;
    let res = s
        .put("/api/v1/auth/password")
        .json(&json!({ "current_password": "password1", "new_password": "\t\t\t\t\t\t\t\t" }))
        .await;
    assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, "change");
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_json_fields_rejected() {
    let app = test_app(&[]).await;
    let res = post(
        &app,
        "/api/v1/auth/signup",
        json!({ "email": "u@x.com", "password": "password1", "name": "Ann", "role": "admin" }),
    )
    .await;
    assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, "signup");
    let (s, uid) = signed_in(&app, "a@x.com", "admin").await;
    let res = s
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com", "user_id": 1 }))
        .await;
    assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, "create link");
    let res = s
        .patch("/api/v1/auth/me")
        .json(&json!({ "name": "Ann", "email": "b@x.com" }))
        .await;
    assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, "me");
    let res = s
        .patch(&format!("/api/v1/admin/users/{uid}"))
        .json(&json!({ "email_verified": true, "admin": true }))
        .await;
    assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, "admin patch");
    let res = s
        .patch(&format!("/api/v1/admin/users/{uid}"))
        .json(&json!({ "email_verified": "yes" }))
        .await;
    assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, "admin patch bool");
}

#[tokio::test(flavor = "multi_thread")]
async fn oversized_body_is_413() {
    let app = test_app(&[]).await;
    let (s, _) = signed_in(&app, "a@x.com", "user").await;
    let url = format!("https://example.com/{}", "a".repeat(17 * 1024));
    let res = s.post("/api/v1/links").json(&json!({ "url": url })).await;
    assert_err(&res, StatusCode::PAYLOAD_TOO_LARGE, "17 KiB");
    let url = format!("https://example.com/{}", "a".repeat(15 * 1024));
    let res = s.post("/api/v1/links").json(&json!({ "url": url })).await;
    assert_err(&res, StatusCode::UNPROCESSABLE_ENTITY, "15 KiB");
}

/// Seeds a live token row whose hash matches `token`, so only a pre-lookup format check can reject it.
async fn plant_token(app: &TestApp, uid: i64, purpose: &str, token: &str) {
    common::exec(
        &app.state.db,
        "INSERT INTO email_tokens (user_id, purpose, token_hash, expires_at) VALUES (?1, ?2, ?3, ?4)",
        libsql::params![uid, purpose, sha256_hex(token), now_unix() + 3600],
    )
    .await;
}

async fn token_used(app: &TestApp, token: &str) -> bool {
    let r = first_row(
        &app.state.db,
        "SELECT used_at IS NOT NULL FROM email_tokens WHERE token_hash = ?1",
        [sha256_hex(token)],
    )
    .await;
    r.get::<i64>(0).unwrap() != 0
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_tokens_rejected_before_lookup() {
    let (app, _) = mail_app(&[]).await;
    // Not via signup: its background verification email would replace planted tokens.
    let uid = insert_user(&app.state.db, "t@x.com", "user").await;
    let short = "A".repeat(42);
    let long = "A".repeat(44);
    // 43 chars but the last one carries non-zero padding bits: never produced by issue_token.
    let noncanonical = format!("{}B", "A".repeat(42));
    for t in [&short, &long, &noncanonical] {
        plant_token(&app, uid, "verify_email", t).await;
        let res = app
            .server
            .post("/api/v1/auth/verify-email")
            .json(&json!({ "token": t }))
            .await;
        assert_err(&res, StatusCode::BAD_REQUEST, t);
        assert_eq!(
            res.json::<Value>()["error"]["message"],
            "invalid or expired token"
        );
        assert!(!token_used(&app, t).await, "{t} reached the DB");
    }
    let reset = "B".repeat(44);
    plant_token(&app, uid, "reset_password", &reset).await;
    let res = post(
        &app,
        "/api/v1/auth/reset-password",
        json!({ "token": reset, "password": "password2" }),
    )
    .await;
    assert_err(&res, StatusCode::BAD_REQUEST, "reset");
    assert!(!token_used(&app, &reset).await);
}

#[tokio::test(flavor = "multi_thread")]
async fn ids_and_cursors_must_be_positive() {
    let app = test_app(&[]).await;
    let (s, _) = signed_in(&app, "a@x.com", "admin").await;
    for path in [
        "/api/v1/admin/users/0",
        "/api/v1/admin/users/-1",
        "/api/v1/links?cursor=0",
        "/api/v1/links?cursor=-5",
        "/api/v1/admin/users?cursor=0",
        "/api/v1/admin/links?user_id=0",
        "/api/v1/admin/links?cursor=-1",
    ] {
        assert_err(&s.get(path).await, StatusCode::BAD_REQUEST, path);
    }
    let res = s
        .patch("/api/v1/admin/users/0")
        .json(&json!({ "email_verified": true }))
        .await;
    assert_err(&res, StatusCode::BAD_REQUEST, "patch user 0");
    assert_err(
        &s.delete("/api/v1/admin/users/-3").await,
        StatusCode::BAD_REQUEST,
        "delete user -3",
    );
    assert_eq!(
        s.get("/api/v1/links/1").await.status_code(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        s.get("/api/v1/links?cursor=1").await.status_code(),
        StatusCode::OK
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn search_query_trimmed_and_capped() {
    let app = test_app(&[]).await;
    let (s, _) = signed_in(&app, "a@x.com", "admin").await;
    s.post("/api/v1/links")
        .json(&json!({ "url": "https://example.com", "slug": "findme" }))
        .await
        .assert_status(StatusCode::CREATED);
    let list: Value = s.get("/api/v1/links?query=%20%20findme%20").await.json();
    assert_eq!(list["links"].as_array().unwrap().len(), 1);
    let ok = "q".repeat(200);
    let bad = "q".repeat(201);
    for base in [
        "/api/v1/links",
        "/api/v1/admin/users",
        "/api/v1/admin/links",
    ] {
        assert_eq!(
            s.get(&format!("{base}?query={ok}")).await.status_code(),
            StatusCode::OK,
            "{base}"
        );
        assert_err(
            &s.get(&format!("{base}?query={bad}")).await,
            StatusCode::UNPROCESSABLE_ENTITY,
            base,
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth_callback_caps_code_and_state() {
    let mut app = test_app(&GOOGLE).await;
    app.server.save_cookies();
    let cb = "/api/v1/auth/oauth/google/callback";
    let state = start_state(&app).await;
    let code = "c".repeat(2049);
    let res = app
        .server
        .get(&format!("{cb}?code={code}&state={state}"))
        .await;
    assert_eq!(res.status_code(), StatusCode::FOUND);
    assert_eq!(res.header("location"), "/login?error=state");
    let long_state = "s".repeat(257);
    let res = app
        .server
        .get(&format!("{cb}?error=access_denied&state={long_state}"))
        .await;
    assert_eq!(res.header("location"), "/login?error=state");
    let res = app
        .server
        .get(&format!(
            "{cb}?error=access_denied&state={}",
            "s".repeat(256)
        ))
        .await;
    assert_eq!(res.header("location"), "/login?error=denied");
}

fn ident(id: &str, email: &str, name: Option<&str>) -> OauthIdentity {
    OauthIdentity {
        provider: OauthProvider::Google,
        provider_user_id: id.into(),
        email: email.into(),
        email_verified: true,
        name: name.map(Into::into),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth_names_normalised_or_dropped() {
    let app = test_app(&[]).await;
    let (db, cfg) = (&app.state.db, &app.state.config);
    let u = resolve_oauth_user(db, cfg, &ident("1", "a@x.com", Some("  Ngô  Bảo ")))
        .await
        .unwrap();
    assert_eq!(u.name.as_deref(), Some("Ngô Bảo"));
    for (i, bad) in [Some("dev123"), Some("😀"), Some(""), None]
        .iter()
        .enumerate()
    {
        let u = resolve_oauth_user(
            db,
            cfg,
            &ident(&format!("b{i}"), &format!("b{i}@x.com"), *bad),
        )
        .await
        .unwrap();
        assert_eq!(u.name, None, "{bad:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn oauth_invalid_email_rejected() {
    let app = test_app(&[]).await;
    let r = resolve_oauth_user(
        &app.state.db,
        &app.state.config,
        &ident("1", "not-an-email", Some("Ann")),
    )
    .await;
    assert_eq!(r.err().unwrap().code(), "email_invalid");
}

/// Gates answer before the body is validated: each body here would be a 422.
#[tokio::test(flavor = "multi_thread")]
async fn route_gates_run_before_body_validation() {
    let bad_signup = json!({ "email": "nope", "password": "x", "name": "" });
    let bad_pw = json!({ "token": "x", "password": "x" });
    let unknown = json!({ "email": "a@x.com", "password": "password1", "extra": 1 });
    let cases: Vec<(TestApp, &str, Value, StatusCode)> = vec![
        (
            test_app(&[("TIINY_SIGNUP_ENABLED", "false")]).await,
            "/api/v1/auth/signup",
            bad_signup.clone(),
            StatusCode::FORBIDDEN,
        ),
        (
            test_app(&[("TIINY_PASSWORD_AUTH_ENABLED", "false")]).await,
            "/api/v1/auth/signup",
            bad_signup,
            StatusCode::NOT_FOUND,
        ),
        (
            test_app(&[("TIINY_PASSWORD_AUTH_ENABLED", "false")]).await,
            "/api/v1/auth/login",
            unknown,
            StatusCode::NOT_FOUND,
        ),
        (
            test_app(&[]).await,
            "/api/v1/auth/forgot-password",
            json!({ "email": "nope" }),
            StatusCode::NOT_FOUND,
        ),
        (
            mail_app(&[("TIINY_PASSWORD_AUTH_ENABLED", "false")])
                .await
                .0,
            "/api/v1/auth/forgot-password",
            json!({ "email": "nope" }),
            StatusCode::NOT_FOUND,
        ),
        (
            test_app(&[]).await,
            "/api/v1/auth/reset-password",
            bad_pw.clone(),
            StatusCode::NOT_FOUND,
        ),
        (
            mail_app(&[("TIINY_PASSWORD_AUTH_ENABLED", "false")])
                .await
                .0,
            "/api/v1/auth/reset-password",
            bad_pw,
            StatusCode::NOT_FOUND,
        ),
        (
            test_app(&[]).await,
            "/api/v1/auth/verify-email",
            json!({ "token": "x", "extra": 1 }),
            StatusCode::NOT_FOUND,
        ),
    ];
    for (app, path, body, want) in cases {
        assert_err(&post(&app, path, body).await, want, path);
    }
    let bad_change = json!({ "new_password": "x" });
    let app = test_app(&[]).await;
    let res = client(&app)
        .put("/api/v1/auth/password")
        .json(&bad_change)
        .await;
    assert_err(&res, StatusCode::UNAUTHORIZED, "change-password signed out");
    let app = test_app(&[("TIINY_PASSWORD_AUTH_ENABLED", "false")]).await;
    let res = client(&app)
        .put("/api/v1/auth/password")
        .json(&bad_change)
        .await;
    assert_err(&res, StatusCode::NOT_FOUND, "change-password disabled");
}
