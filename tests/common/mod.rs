#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use garde::{Unvalidated, Valid};
use tiiny::{
    AppState,
    auth::email::{Mailer, SentMail},
    build_router,
    config::Config,
    db::{Db, now_unix},
    links::model::{Link, LinkEdit, NewLink, UserRef, create_link},
};

pub struct TestApp {
    pub server: axum_test::TestServer,
    pub state: AppState,
    pub _dir: tempfile::TempDir,
}

/// Test base URL and `dir` as the data dir; `vars` add to or override them.
pub fn config(dir: &std::path::Path, vars: &[(&str, &str)]) -> Config {
    let dir = dir.to_string_lossy();
    let mut all = vec![
        ("TIINY_BASE_URL", "http://tiiny.test"),
        ("TIINY_DATA_DIR", &*dir),
    ];
    all.retain(|(k, _)| !vars.iter().any(|(vk, _)| vk == k));
    all.extend_from_slice(vars);
    Config::from_vars(all.into_iter().map(|(k, v)| (k.into(), v.into()))).unwrap()
}

pub const GOOGLE: [(&str, &str); 2] = [
    ("TIINY_GOOGLE_CLIENT_ID", "gid"),
    ("TIINY_GOOGLE_CLIENT_SECRET", "gsecret"),
];

pub async fn test_app(vars: &[(&str, &str)]) -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path(), vars);
    let db = Db::open(&config).await.unwrap();
    db.migrate().await.unwrap();
    let state = AppState::new(config, db).unwrap();
    let server = json_server(&state, false);
    TestApp {
        server,
        state,
        _dir: dir,
    }
}

pub async fn mail_app(vars: &[(&str, &str)]) -> (TestApp, Arc<Mutex<Vec<SentMail>>>) {
    let mut v = vec![
        ("TIINY_SMTP_HOST", "smtp.test"),
        ("TIINY_SMTP_FROM", "tiiny@tiiny.test"),
    ];
    v.extend_from_slice(vars);
    let mut app = test_app(&v).await;
    let sent = Arc::new(Mutex::new(vec![]));
    app.state.mailer = Mailer::Capture(Arc::clone(&sent));
    app.server = json_server(&app.state, true);
    (app, sent)
}

/// Same as the crate's token hash.
pub fn sha256_hex(s: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Runs one write statement as its own unit.
pub async fn exec(db: &Db, sql: &str, params: impl libsql::params::IntoParams) -> u64 {
    db.conn().await.unwrap().execute(sql, params).await.unwrap()
}

pub async fn sql(app: &TestApp, q: &str, p: impl libsql::params::IntoParams) {
    exec(&app.state.db, q, p).await;
}

/// First row, read from the replica.
pub async fn first_row(db: &Db, sql: &str, params: impl libsql::params::IntoParams) -> libsql::Row {
    let conn = db.conn().await.unwrap();
    let mut rows = conn.query(sql, params).await.unwrap();
    rows.next().await.unwrap().unwrap()
}

pub async fn scalar(db: &Db, sql: &str, params: impl libsql::params::IntoParams) -> i64 {
    first_row(db, sql, params).await.get(0).unwrap()
}

pub async fn insert_user(db: &Db, email: &str, role: &str) -> i64 {
    let w = db.conn().await.unwrap();
    w.execute(
        "INSERT INTO users (email, role, created_at) VALUES (?1, ?2, ?3)",
        libsql::params![email, role, now_unix()],
    )
    .await
    .unwrap();
    w.last_insert_rowid()
}

/// A link for a non-admin user, created as `POST /links` creates it; `permanent` then patches it as `PATCH /links/{slug}` does.
pub async fn add_link(app: &TestApp, uid: i64, slug: &str, url: &str, permanent: bool) -> Link {
    let (db, cfg) = (&app.state.db, &app.state.config);
    let user = UserRef {
        id: uid,
        is_admin: false,
    };
    let link = create_link(db, cfg, &user, new_link(cfg, url, Some(slug)))
        .await
        .unwrap();
    if !permanent {
        return link;
    }
    tiiny::links::model::update_link(db, link.id, link_edit(cfg, None, None, Some(true)))
        .await
        .unwrap()
        .0
}

/// A [`test_app`] whose client keeps cookies across requests.
pub async fn cookie_app(vars: &[(&str, &str)]) -> TestApp {
    let mut app = test_app(vars).await;
    app.server.save_cookies();
    app
}

pub async fn signup(s: &axum_test::TestServer, email: &str) -> axum_test::TestResponse {
    s.post("/api/v1/auth/signup")
        .json(&serde_json::json!({ "email": email, "password": "password1", "name": "Test User" }))
        .await
}

pub async fn start_state(app: &TestApp) -> String {
    let res = app.server.get("/api/v1/auth/oauth/google/start").await;
    let loc = url::Url::parse(res.header("location").to_str().unwrap()).unwrap();
    loc.query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned()
}

/// Like the SPA's api-client: every request carries `Content-Type: application/json`.
pub fn json_server(state: &AppState, save_cookies: bool) -> axum_test::TestServer {
    let b = axum_test::TestServer::builder().default_content_type("application/json");
    let b = if save_cookies { b.save_cookies() } else { b };
    b.build(build_router(state.clone()))
}

/// `name=value` of the `tiiny_guest` Set-Cookie, ready for a `Cookie` header.
pub fn guest_cookie(resp: &axum_test::TestResponse) -> String {
    resp.header("set-cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

/// A separate cookie jar on the same app state.
pub fn client(app: &TestApp) -> axum_test::TestServer {
    json_server(&app.state, true)
}

/// No default content type.
pub fn raw_client(app: &TestApp) -> axum_test::TestServer {
    let mut s = axum_test::TestServer::new(build_router(app.state.clone()));
    s.save_cookies();
    s
}

pub async fn signed_in(app: &TestApp, email: &str, role: &str) -> (axum_test::TestServer, i64) {
    let s = client(app);
    let res = signup(&s, email).await;
    assert_eq!(res.status_code(), 201, "{}", res.text());
    let id = res.json::<serde_json::Value>()["id"].as_i64().unwrap();
    exec(
        &app.state.db,
        "UPDATE users SET role = ?2 WHERE id = ?1",
        libsql::params![id, role],
    )
    .await;
    (s, id)
}

/// A link body validated exactly as `POST /links` validates it.
pub fn new_link(cfg: &Arc<Config>, url: &str, slug: Option<&str>) -> Valid<NewLink> {
    Unvalidated::new(NewLink {
        url: url.into(),
        slug: slug.map(Into::into),
    })
    .validate_with(cfg)
    .unwrap()
}

/// An edit validated exactly as `PATCH /links/{slug}` validates it.
pub fn link_edit(
    cfg: &Arc<Config>,
    url: Option<&str>,
    slug: Option<&str>,
    permanent: Option<bool>,
) -> Valid<LinkEdit> {
    Unvalidated::new(LinkEdit {
        url: url.map(Into::into),
        slug: slug.map(Into::into),
        permanent,
    })
    .validate_with(cfg)
    .unwrap()
}
