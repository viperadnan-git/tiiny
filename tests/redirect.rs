mod common;

use common::{add_link, insert_user, link_edit, test_app};
use tiiny::links::model::{set_blocked, update_link};

#[tokio::test(flavor = "multi_thread")]
async fn redirect_behaviour() {
    let app = test_app(&[]).await;
    let uid = insert_user(&app.state.db, "a@x.com", "user").await;
    let db = &app.state.db;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;

    let r = app.server.get("/abc").await;
    r.assert_status(axum::http::StatusCode::FOUND);
    assert_eq!(r.header("location"), "https://example.com/x");
    assert_eq!(r.header("cache-control"), "private, no-cache");

    app.server
        .get("/abc?utm=1")
        .await
        .assert_status(axum::http::StatusCode::FOUND);
    for p in ["/zzz9", "/abc/", "/ab", "/%F0%9F%98%80", "/a/b", "/ABC"] {
        app.server
            .get(p)
            .await
            .assert_status(axum::http::StatusCode::NOT_FOUND);
    }
    app.server.get("/healthz").await.assert_text("ok");

    let (l, old) = update_link(
        db,
        1,
        link_edit(&app.state.config, None, Some("newslug"), None),
    )
    .await
    .unwrap();
    assert_eq!(l.slug, "newslug");
    app.state.cache.evict(&old);
    app.server
        .get("/abc")
        .await
        .assert_status(axum::http::StatusCode::NOT_FOUND);
    app.server
        .get("/newslug")
        .await
        .assert_status(axum::http::StatusCode::FOUND);

    common::exec(db, "UPDATE users SET disabled_at = 1 WHERE id = ?1", [uid]).await;
    app.state.cache.evict_all();
    app.server
        .get("/newslug")
        .await
        .assert_status(axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn permanent_redirect_and_gone() {
    use axum::http::StatusCode;
    use tiiny::links::model::delete_link;
    let app = test_app(&[]).await;
    let uid = insert_user(&app.state.db, "a@x.com", "user").await;
    let db = &app.state.db;
    let p = add_link(&app, uid, "perm", "https://example.com/p", true).await;
    app.state.cache.evict("perm");
    let r = app.server.get("/perm").await;
    r.assert_status(StatusCode::MOVED_PERMANENTLY);
    assert_eq!(r.header("location"), "https://example.com/p");
    assert_eq!(r.header("cache-control"), "private, max-age=86400");

    add_link(&app, uid, "temp", "https://example.com/t", false).await;
    app.server
        .get("/temp")
        .await
        .assert_status(StatusCode::FOUND);

    delete_link(db, p.id).await.unwrap();
    app.state.cache.evict("perm");
    let r = app.server.get("/perm").await;
    r.assert_status(StatusCode::GONE);
}

#[tokio::test(flavor = "multi_thread")]
async fn guest_link_redirects_302() {
    let app = test_app(&[]).await;
    common::exec(
        &app.state.db,
        "INSERT INTO links (user_id, guest_key_hash, slug, url, created_at, updated_at) \
         VALUES (NULL, 'h', 'guest01', 'https://example.com/g', 1, 1)",
        (),
    )
    .await;
    let r = app.server.get("/guest01").await;
    r.assert_status(axum::http::StatusCode::FOUND);
    assert_eq!(r.header("location"), "https://example.com/g");
}

#[tokio::test(flavor = "multi_thread")]
async fn blocked_link_answers_451() {
    use axum::http::StatusCode;
    let app = test_app(&[]).await;
    let uid = insert_user(&app.state.db, "a@x.com", "user").await;
    let l = add_link(&app, uid, "bad", "https://example.com/b", false).await;
    app.server
        .get("/bad")
        .await
        .assert_status(StatusCode::FOUND);

    set_blocked(&app.state.db, l.id, true).await.unwrap();
    app.state.cache.evict("bad");
    let r = app.server.get("/bad").await;
    r.assert_status(StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS);
    assert!(r.text().contains("disabled for abuse"));

    set_blocked(&app.state.db, l.id, false).await.unwrap();
    app.state.cache.evict("bad");
    app.server
        .get("/bad")
        .await
        .assert_status(StatusCode::FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn blocked_link_records_no_click() {
    let app = test_app(&[]).await;
    let uid = insert_user(&app.state.db, "a@x.com", "user").await;
    let l = add_link(&app, uid, "bad", "https://example.com/b", false).await;
    set_blocked(&app.state.db, l.id, true).await.unwrap();
    app.server
        .get("/bad")
        .await
        .assert_status(axum::http::StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS);
    app.state.stats.flush(&app.state.db).await.unwrap();
    let n = common::scalar(
        &app.state.db,
        "SELECT click_count FROM links WHERE id = ?1",
        [l.id],
    )
    .await;
    assert_eq!(n, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn plus_previews_without_redirecting_or_counting() {
    use axum::http::StatusCode;
    let app = test_app(&[]).await;
    let uid = insert_user(&app.state.db, "a@x.com", "user").await;
    let l = add_link(
        &app,
        uid,
        "peek",
        "https://example.com/a?x=1&y=\"<b>\"",
        false,
    )
    .await;
    add_link(&app, uid, "perm", "https://example.org/", true).await;

    let r = app.server.get("/peek+").await;
    r.assert_status_ok();
    assert!(r.maybe_header("location").is_none());
    assert_eq!(r.header("cache-control"), "private, no-cache");
    let html = r.text();
    assert!(html.contains(">example.com<"), "{html}");
    assert!(html.contains("href=\"https://example.com/a?x=1&amp;y=%22%3Cb%3E%22\""));
    assert!(!html.contains("%%TIINY_PREVIEW_") && !html.contains("<b>"));
    assert!(app.server.get("/perm+").await.text().contains(">301<"));

    app.state.stats.flush(&app.state.db).await.unwrap();
    let clicks = common::scalar(
        &app.state.db,
        "SELECT click_count FROM links WHERE id = ?1",
        [l.id],
    )
    .await;
    assert_eq!(clicks, 0, "a preview is not a click");

    set_blocked(&app.state.db, l.id, true).await.unwrap();
    app.state.cache.evict("peek");
    app.server
        .get("/peek+")
        .await
        .assert_status(StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS);
    for p in ["/nope+", "/peek++", "/+", "/preview"] {
        app.server.get(p).await.assert_status(StatusCode::NOT_FOUND);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn head_redirects_without_counting() {
    let app = test_app(&[]).await;
    let uid = insert_user(&app.state.db, "a@x.com", "user").await;
    let l = add_link(&app, uid, "head", "https://example.com/h", false).await;
    let r = app.server.method(axum::http::Method::HEAD, "/head").await;
    r.assert_status(axum::http::StatusCode::FOUND);
    assert_eq!(r.header("location"), "https://example.com/h");
    app.server.get("/head").await;
    app.state.stats.flush(&app.state.db).await.unwrap();
    let clicks = common::scalar(
        &app.state.db,
        "SELECT click_count FROM links WHERE id = ?1",
        [l.id],
    )
    .await;
    assert_eq!(clicks, 1, "only the GET counts");
}
