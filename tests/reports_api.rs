mod common;

use axum::http::StatusCode;
use common::{add_link, insert_user, scalar, test_app};
use serde_json::{Value, json};

async fn report(app: &common::TestApp, body: Value) -> axum_test::TestResponse {
    app.server.post("/api/v1/reports").json(&body).await
}

fn msg(res: &axum_test::TestResponse) -> String {
    res.json::<Value>()["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn report_by_slug() {
    let app = test_app(&[]).await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    let link = add_link(&app, uid, "abc", "https://example.com/x", false).await;
    report(
        &app,
        json!({ "slug": "abc", "reason": "spam", "details": " bad ", "contact_email": "A@B.co" }),
    )
    .await
    .assert_status(StatusCode::CREATED);
    report(
        &app,
        json!({ "slug": "abc", "reason": "other", "contact_email": "" }),
    )
    .await
    .assert_status(StatusCode::CREATED);
    let n = scalar(
        &app.state.db,
        "SELECT COUNT(*) FROM abuse_reports WHERE status = 'open' AND link_id = ?1 AND slug = 'abc' AND url = 'https://example.com/x'",
        [link.id],
    )
    .await;
    assert_eq!(n, 2);
    let n = scalar(
        &app.state.db,
        "SELECT COUNT(*) FROM abuse_reports WHERE details = 'bad' AND contact_email = 'a@b.co'",
        (),
    )
    .await;
    assert_eq!(n, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn report_slug_rejected() {
    let app = test_app(&[]).await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com", false).await;
    for bad in [
        "https://tiiny.test/abc",
        "tiiny.test/abc",
        "  abc  ",
        "ab",
        "",
    ] {
        let res = report(&app, json!({ "slug": bad, "reason": "spam" })).await;
        res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(msg(&res), "slug doesn't match any link", "{bad}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn report_unknown_disabled_or_blocked() {
    let app = test_app(&[]).await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    let dis = insert_user(&app.state.db, "d@x.com", "user").await;
    add_link(&app, uid, "blk", "https://example.com", false).await;
    add_link(&app, dis, "off", "https://example.com", false).await;
    common::sql(
        &app,
        "UPDATE links SET blocked_at = 1 WHERE slug = 'blk'",
        (),
    )
    .await;
    common::sql(
        &app,
        "UPDATE users SET disabled_at = 1 WHERE id = ?1",
        [dis],
    )
    .await;
    for slug in ["nope", "off"] {
        let res = report(&app, json!({ "slug": slug, "reason": "spam" })).await;
        res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(msg(&res), "slug doesn't match any link");
    }
    report(&app, json!({ "slug": "blk", "reason": "spam" }))
        .await
        .assert_status(StatusCode::CREATED);
}

#[tokio::test(flavor = "multi_thread")]
async fn report_validation() {
    let app = test_app(&[]).await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com", false).await;
    let cases = [
        (
            json!({ "slug": "abc", "reason": "x" }),
            "reason must be one of phishing, malware, spam, illegal, other",
        ),
        (
            json!({ "slug": "abc", "reason": "spam", "details": "a".repeat(2001) }),
            "details must be at most 2000 characters",
        ),
        (
            json!({ "slug": "abc", "reason": "spam", "contact_email": "nope" }),
            "contact email is invalid",
        ),
        (json!({ "slug": "abc", "reason": "spam", "extra": 1 }), ""),
    ];
    for (body, want) in cases {
        let res = report(&app, body).await;
        res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        if !want.is_empty() {
            assert_eq!(msg(&res), want);
        }
    }
    report(
        &app,
        json!({ "slug": "abc", "reason": "spam", "details": "a".repeat(2000) }),
    )
    .await
    .assert_status(StatusCode::CREATED);
}

#[tokio::test(flavor = "multi_thread")]
async fn report_rate_limited() {
    let app = test_app(&[]).await;
    let mut limited = false;
    for _ in 0..11 {
        let res = report(&app, json!({ "slug": "nope", "reason": "spam" })).await;
        limited |= res.status_code() == StatusCode::TOO_MANY_REQUESTS;
    }
    assert!(limited);
}

async fn file(app: &common::TestApp, slug: &str) {
    report(app, json!({ "slug": slug, "reason": "spam" }))
        .await
        .assert_status(StatusCode::CREATED);
}

async fn open_ids(app: &common::TestApp, admin: &axum_test::TestServer) -> Vec<i64> {
    let _ = app;
    let v: Value = admin.get("/api/v1/admin/reports").await.json();
    v["reports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_i64().unwrap())
        .collect()
}

async fn resolve(admin: &axum_test::TestServer, id: i64, action: &str) -> axum_test::TestResponse {
    admin
        .post(&format!("/api/v1/admin/reports/{id}/resolve"))
        .json(&json!({ "action": action }))
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_lists_open_reports() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    file(&app, "abc").await;
    file(&app, "abc").await;
    let v: Value = admin.get("/api/v1/admin/reports").await.json();
    let r = &v["reports"][0];
    assert_eq!(v["reports"].as_array().unwrap().len(), 2);
    assert_eq!(r["open_for_link"], 2);
    assert_eq!(r["status"], "open");
    assert_eq!(r["link"]["slug"], "abc");
    assert_eq!(r["link"]["owner_email"], "u@x.com");
    assert_eq!(r["link"]["blocked"], false);
    let c: Value = admin.get("/api/v1/admin/reports/count").await.json();
    assert_eq!(c, json!({ "open": 2 }));
    admin
        .get("/api/v1/admin/reports?status=bogus")
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let v: Value = admin
        .get("/api/v1/admin/reports?status=actioned")
        .await
        .json();
    assert!(v["reports"].as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn resolve_block_groups() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    app.server.get("/abc").await;
    file(&app, "abc").await;
    file(&app, "abc").await;
    let ids = open_ids(&app, &admin).await;
    resolve(&admin, ids[0], "block")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let n = scalar(
        &app.state.db,
        "SELECT COUNT(*) FROM abuse_reports WHERE status = 'actioned' AND action = 'blocked' AND resolved_by IS NOT NULL AND resolved_at IS NOT NULL",
        (),
    )
    .await;
    assert_eq!(n, 2);
    assert_eq!(app.server.get("/abc").await.status_code(), 451);
    let c: Value = admin.get("/api/v1/admin/reports/count").await.json();
    assert_eq!(c["open"], 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn resolve_delete() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", true).await;
    app.server.get("/abc").await;
    file(&app, "abc").await;
    file(&app, "abc").await;
    let ids = open_ids(&app, &admin).await;
    resolve(&admin, ids[0], "delete")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let n = scalar(
        &app.state.db,
        "SELECT COUNT(*) FROM abuse_reports WHERE status = 'actioned' AND action = 'deleted' AND link_id IS NULL",
        (),
    )
    .await;
    assert_eq!(n, 2);
    assert_eq!(app.server.get("/abc").await.status_code(), 410);
}

#[tokio::test(flavor = "multi_thread")]
async fn resolve_disable_owner() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    file(&app, "abc").await;
    let ids = open_ids(&app, &admin).await;
    resolve(&admin, ids[0], "disable_owner")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let n = scalar(
        &app.state.db,
        "SELECT COUNT(*) FROM users WHERE id = ?1 AND disabled_at IS NOT NULL",
        [uid],
    )
    .await;
    assert_eq!(n, 1);
    let n = scalar(
        &app.state.db,
        "SELECT COUNT(*) FROM abuse_reports WHERE status = 'actioned' AND action = 'owner_disabled'",
        (),
    )
    .await;
    assert_eq!(n, 1);
    let n = scalar(
        &app.state.db,
        "SELECT COUNT(*) FROM links WHERE slug = 'abc' AND blocked_at IS NOT NULL",
        (),
    )
    .await;
    assert_eq!(n, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn resolve_dismiss_single() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    file(&app, "abc").await;
    file(&app, "abc").await;
    let ids = open_ids(&app, &admin).await;
    resolve(&admin, ids[0], "dismiss")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let c: Value = admin.get("/api/v1/admin/reports/count").await.json();
    assert_eq!(c["open"], 1);
    assert_eq!(app.server.get("/abc").await.status_code(), 302);
    let v: Value = admin
        .get("/api/v1/admin/reports?status=dismissed")
        .await
        .json();
    assert_eq!(v["reports"][0]["status"], "dismissed");
}

#[tokio::test(flavor = "multi_thread")]
async fn disable_owner_on_guest_link_422() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let g = app
        .server
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com/g" }))
        .await;
    let slug = g.json::<Value>()["slug"].as_str().unwrap().to_owned();
    file(&app, &slug).await;
    let ids = open_ids(&app, &admin).await;
    let res = resolve(&admin, ids[0], "disable_owner").await;
    res.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(msg(&res), "guest links have no owner to disable");
    resolve(&admin, ids[0], "block")
        .await
        .assert_status(StatusCode::NO_CONTENT);
}

#[tokio::test(flavor = "multi_thread")]
async fn resolve_twice_409() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    file(&app, "abc").await;
    let ids = open_ids(&app, &admin).await;
    resolve(&admin, ids[0], "dismiss")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    resolve(&admin, ids[0], "block")
        .await
        .assert_status(StatusCode::CONFLICT);
    resolve(&admin, 9999, "block")
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn non_admin_403() {
    let app = test_app(&[]).await;
    let (u, _) = common::signed_in(&app, "u@x.com", "user").await;
    u.get("/api/v1/admin/reports")
        .await
        .assert_status(StatusCode::FORBIDDEN);
    u.get("/api/v1/admin/reports/count")
        .await
        .assert_status(StatusCode::FORBIDDEN);
    resolve(&u, 1, "dismiss")
        .await
        .assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_survive_owner_deletion() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    file(&app, "abc").await;
    file(&app, "abc").await;
    admin
        .delete(&format!("/api/v1/admin/users/{uid}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let v: Value = admin.get("/api/v1/admin/reports").await.json();
    assert_eq!(v["reports"].as_array().unwrap().len(), 2);
    assert!(v["reports"][0]["link"].is_null());
    assert_eq!(v["reports"][0]["open_for_link"], 0);
    let ids = open_ids(&app, &admin).await;
    resolve(&admin, ids[0], "block")
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    resolve(&admin, ids[0], "dismiss")
        .await
        .assert_status(StatusCode::NO_CONTENT);
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_reports_search() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "owner@x.test", "user").await;
    let other = insert_user(&app.state.db, "o2@x.test", "user").await;
    add_link(&app, uid, "aaa", "https://example.com/a", false).await;
    add_link(&app, other, "bbb", "https://example.com/b", false).await;
    add_link(&app, other, "c_c", "https://example.com/c", false).await;
    report(
        &app,
        json!({ "slug": "bbb", "reason": "spam", "details": "crypto giveaway" }),
    )
    .await;
    file(&app, "aaa").await;
    file(&app, "c_c").await;
    common::sql(
        &app,
        "UPDATE abuse_reports SET status = 'dismissed' WHERE slug = 'c_c'",
        (),
    )
    .await;

    let slugs = |v: Value| -> Vec<String> {
        v["reports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["slug"].as_str().unwrap().to_owned())
            .collect()
    };
    let get = |q: &'static str| {
        let admin = &admin;
        async move {
            slugs(
                admin
                    .get(&format!("/api/v1/admin/reports?{q}"))
                    .await
                    .json(),
            )
        }
    };
    assert_eq!(get("query=giveaway").await, ["bbb"]);
    assert_eq!(get("query=owner@x").await, ["aaa"]);
    assert_eq!(get("query=").await.len(), 2);
    assert_eq!(get("query=c_c&status=dismissed").await, ["c_c"]);
    assert!(get("query=c_c").await.is_empty());
    assert!(get("query=%25&status=dismissed").await.is_empty());
    assert!(get("query=c%25c&status=dismissed").await.is_empty());
    assert_eq!(get("query=b&limit=1").await.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn notifier_sends_one_summary() {
    let (app, sent) = common::mail_app(&[]).await;
    insert_user(&app.state.db, "a1@x.com", "admin").await;
    insert_user(&app.state.db, "a2@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    for _ in 0..3 {
        report(&app, json!({ "slug": "abc", "reason": "spam" }))
            .await
            .assert_status(StatusCode::CREATED);
    }
    let after = tiiny::reports::notify::notify_once(&app.state, 0)
        .await
        .unwrap();
    assert!(after > 0);
    {
        let m = sent.lock().unwrap();
        assert_eq!(m.len(), 1, "one message for all admins");
        assert_eq!(m[0].to, "a1@x.com, a2@x.com");
        assert_eq!(m[0].subject, "3 new abuse reports");
        assert!(
            m[0].body
                .contains("http://tiiny.test/abc → https://example.com/x (spam)")
        );
        assert!(
            m[0].body
                .contains("Review: http://tiiny.test/admin/reports")
        );
    }
    assert_eq!(
        tiiny::reports::notify::notify_once(&app.state, after)
            .await
            .unwrap(),
        after
    );
    assert_eq!(sent.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn notifier_skips_without_mailer() {
    let app = test_app(&[]).await;
    assert_eq!(
        tiiny::reports::notify::notify_once(&app.state, 7)
            .await
            .unwrap(),
        7
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn resolve_delete_retires_temporary_slug() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    file(&app, "abc").await;
    let ids = open_ids(&app, &admin).await;
    resolve(&admin, ids[0], "delete")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert_eq!(app.server.get("/abc").await.status_code(), 410);
    let res = admin
        .post("/api/v1/links")
        .json(&json!({ "url": "https://example.com", "slug": "abc" }))
        .await;
    res.assert_status(StatusCode::CONFLICT);
    assert_eq!(
        res.json::<Value>()["error"]["message"],
        "slug is already taken. Try another"
    );
}

async fn reopen(admin: &axum_test::TestServer, id: i64) -> axum_test::TestResponse {
    admin
        .post(&format!("/api/v1/admin/reports/{id}/reopen"))
        .json(&json!({}))
        .await
}

async fn open_count(admin: &axum_test::TestServer) -> i64 {
    admin
        .get("/api/v1/admin/reports/count")
        .await
        .json::<Value>()["open"]
        .as_i64()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn reopen_dismissed_reopens_only_that_report() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    file(&app, "abc").await;
    file(&app, "abc").await;
    let ids = open_ids(&app, &admin).await;
    for id in &ids {
        resolve(&admin, *id, "dismiss")
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    let res = reopen(&admin, ids[0]).await;
    res.assert_status_ok();
    assert_eq!(res.json::<Value>()["reopened"], 1);
    assert_eq!(open_count(&admin).await, 1);
    let n = scalar(
        &app.state.db,
        "SELECT COUNT(*) FROM abuse_reports WHERE status = 'open' AND action IS NULL AND resolved_by IS NULL AND resolved_at IS NULL",
        (),
    )
    .await;
    assert_eq!(n, 1);
    let res = reopen(&admin, ids[0]).await;
    res.assert_status(StatusCode::CONFLICT);
    assert_eq!(msg(&res), "this report is already open");
}

#[tokio::test(flavor = "multi_thread")]
async fn undo_block_unblocks_and_reopens_the_group() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    file(&app, "abc").await;
    file(&app, "abc").await;
    let ids = open_ids(&app, &admin).await;
    resolve(&admin, ids[0], "block")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert_eq!(app.server.get("/abc").await.status_code(), 451);
    let res = reopen(&admin, ids[1]).await;
    res.assert_status_ok();
    assert_eq!(res.json::<Value>()["reopened"], 2);
    assert_eq!(open_count(&admin).await, 2);
    // Cache evicted: the redirect works again right away.
    assert_eq!(app.server.get("/abc").await.status_code(), 302);
}

#[tokio::test(flavor = "multi_thread")]
async fn undo_disable_owner_enables_them_again() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    add_link(&app, uid, "other", "https://example.com/y", false).await;
    file(&app, "abc").await;
    let ids = open_ids(&app, &admin).await;
    resolve(&admin, ids[0], "disable_owner")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert_eq!(app.server.get("/other").await.status_code(), 404);
    reopen(&admin, ids[0]).await.assert_status_ok();
    let disabled = scalar(
        &app.state.db,
        "SELECT COUNT(*) FROM users WHERE id = ?1 AND disabled_at IS NOT NULL",
        [uid],
    )
    .await;
    assert_eq!(disabled, 0);
    assert_eq!(app.server.get("/abc").await.status_code(), 302);
    assert_eq!(app.server.get("/other").await.status_code(), 302);
    assert_eq!(open_count(&admin).await, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn reopen_keeps_a_block_and_disable_made_before_the_report() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    file(&app, "abc").await;
    file(&app, "abc").await;
    let ids = open_ids(&app, &admin).await;
    // Blocked and disabled by hand first; the reports are actioned afterwards.
    admin
        .patch("/api/v1/admin/links/abc")
        .json(&json!({ "blocked": true }))
        .await
        .assert_status_ok();
    admin
        .patch(&format!("/api/v1/admin/users/{uid}"))
        .json(&json!({ "disabled": true }))
        .await
        .assert_status_ok();
    resolve(&admin, ids[0], "disable_owner")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    reopen(&admin, ids[0]).await.assert_status_ok();
    assert_eq!(open_count(&admin).await, 2);
    let still = scalar(
        &app.state.db,
        "SELECT (SELECT COUNT(*) FROM users WHERE id = ?1 AND disabled_at IS NOT NULL)
              + (SELECT COUNT(*) FROM links WHERE slug = 'abc' AND blocked_at IS NOT NULL)",
        [uid],
    )
    .await;
    assert_eq!(still, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_link_reports_cannot_be_reopened() {
    let app = test_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    file(&app, "abc").await;
    let ids = open_ids(&app, &admin).await;
    resolve(&admin, ids[0], "delete")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let res = reopen(&admin, ids[0]).await;
    res.assert_status(StatusCode::CONFLICT);
    assert!(msg(&res).contains("can't be restored"), "{}", msg(&res));
    assert_eq!(open_count(&admin).await, 0);
    reopen(&admin, 9999)
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn notifier_leaves_out_reports_already_acted_on() {
    let (app, sent) = common::mail_app(&[]).await;
    let (admin, _) = common::signed_in(&app, "admin@x.com", "admin").await;
    let uid = insert_user(&app.state.db, "u@x.com", "user").await;
    add_link(&app, uid, "abc", "https://example.com/x", false).await;
    add_link(&app, uid, "def", "https://example.com/y", false).await;
    file(&app, "abc").await;
    file(&app, "abc").await;
    file(&app, "def").await;
    let ids = open_ids(&app, &admin).await;
    // The oldest report is on /abc; blocking closes both /abc reports before the email goes out.
    resolve(&admin, *ids.iter().min().unwrap(), "block")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    // Signing the admin up sent a verification email; count report emails only.
    let reports_sent = || {
        sent.lock()
            .unwrap()
            .iter()
            .filter(|m| m.subject.contains("abuse report"))
            .cloned()
            .collect::<Vec<_>>()
    };
    let after = tiiny::reports::notify::notify_once(&app.state, 0)
        .await
        .unwrap();
    let m = reports_sent();
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].subject, "1 new abuse report");
    assert!(m[0].body.contains("/def"), "{}", m[0].body);
    assert!(!m[0].body.contains("/abc"), "{}", m[0].body);
    assert_eq!(
        after,
        *ids.iter().max().unwrap(),
        "cursor passes resolved reports too"
    );
    // Everything new is already resolved: no email, cursor still moves on.
    file(&app, "def").await;
    let newest = *open_ids(&app, &admin).await.iter().max().unwrap();
    resolve(&admin, newest, "dismiss")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let after2 = tiiny::reports::notify::notify_once(&app.state, after)
        .await
        .unwrap();
    assert_eq!(reports_sent().len(), 1);
    assert_eq!(after2, newest);
}
