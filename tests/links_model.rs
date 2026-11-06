mod common;

use common::{insert_user, link_edit, new_link, test_app};
use tiiny::{
    error::AppError,
    links::model::{
        LinkScope, UserRef, create_link, delete_link, find_link, list_links, update_link,
    },
};

fn user(id: i64) -> UserRef {
    UserRef {
        id,
        is_admin: false,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn create_and_conflict() {
    let app = test_app(&[]).await;
    let (db, cfg) = (&app.state.db, &app.state.config);
    let u = user(insert_user(db, "a@x.com", "user").await);
    let l = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", Some("my-slug"), false),
    )
    .await
    .unwrap();
    assert_eq!(l.slug, "my-slug");
    assert_eq!(
        find_link(db, l.id).await.unwrap().unwrap().url,
        "https://example.com/"
    );
    let dup = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", Some("my-slug"), false),
    )
    .await;
    assert!(matches!(dup, Err(AppError::Conflict(_))));
    let r = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", None, false),
    )
    .await
    .unwrap();
    assert_eq!(r.slug.len(), 7);
}

#[tokio::test(flavor = "multi_thread")]
async fn daily_limit() {
    let app = test_app(&[("TIINY_MAX_LINKS_PER_USER_PER_DAY", "2")]).await;
    let (db, cfg) = (&app.state.db, &app.state.config);
    let u = user(insert_user(db, "a@x.com", "user").await);
    for _ in 0..2 {
        create_link(
            db,
            cfg,
            &u,
            new_link(cfg, "https://example.com", None, false),
        )
        .await
        .unwrap();
    }
    let third = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", None, false),
    )
    .await;
    assert!(matches!(third, Err(AppError::TooManyRequests(_))));
    let admin = UserRef {
        id: insert_user(db, "b@x.com", "admin").await,
        is_admin: true,
    };
    for _ in 0..3 {
        create_link(
            db,
            cfg,
            &admin,
            new_link(cfg, "https://example.com", None, false),
        )
        .await
        .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_does_not_refund_daily_limit() {
    let app = test_app(&[("TIINY_MAX_LINKS_PER_USER_PER_DAY", "1")]).await;
    let (db, cfg) = (&app.state.db, &app.state.config);
    let u = user(insert_user(db, "a@x.com", "user").await);
    let taken = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", Some("taken"), false),
    )
    .await
    .unwrap();
    delete_link(db, taken.id).await.unwrap();
    let again = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", None, false),
    )
    .await;
    assert!(matches!(again, Err(AppError::TooManyRequests(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_create_does_not_use_daily_limit() {
    let app = test_app(&[("TIINY_MAX_LINKS_PER_USER_PER_DAY", "1")]).await;
    let (db, cfg) = (&app.state.db, &app.state.config);
    let a = user(insert_user(db, "a@x.com", "user").await);
    let b = user(insert_user(db, "b@x.com", "user").await);
    create_link(
        db,
        cfg,
        &a,
        new_link(cfg, "https://example.com", Some("dup"), false),
    )
    .await
    .unwrap();
    let dup = create_link(
        db,
        cfg,
        &b,
        new_link(cfg, "https://example.com", Some("dup"), false),
    )
    .await;
    assert!(matches!(dup, Err(AppError::Conflict(_))));
    create_link(
        db,
        cfg,
        &b,
        new_link(cfg, "https://example.com", None, false),
    )
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn yesterday_does_not_count() {
    let app = test_app(&[("TIINY_MAX_LINKS_PER_USER_PER_DAY", "1")]).await;
    let (db, cfg) = (&app.state.db, &app.state.config);
    let u = user(insert_user(db, "a@x.com", "user").await);
    let l = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", None, false),
    )
    .await
    .unwrap();
    common::exec(
        db,
        "UPDATE link_quota SET epoch_day = epoch_day - 1 WHERE owner = 'u:' || ?1",
        [l.user_id],
    )
    .await;
    create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", None, false),
    )
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn listing() {
    let app = test_app(&[]).await;
    let (db, cfg) = (&app.state.db, &app.state.config);
    let a = insert_user(db, "a@x.com", "user").await;
    let b = insert_user(db, "b@x.com", "user").await;
    let l1 = create_link(
        db,
        cfg,
        &user(a),
        new_link(cfg, "https://one.com", Some("first"), false),
    )
    .await
    .unwrap();
    let l2 = create_link(
        db,
        cfg,
        &user(a),
        new_link(cfg, "https://two.com", Some("second"), false),
    )
    .await
    .unwrap();
    let l3 = create_link(
        db,
        cfg,
        &user(b),
        new_link(cfg, "https://three.com", Some("third"), false),
    )
    .await
    .unwrap();
    let ids = |v: Vec<tiiny::links::model::Link>| v.into_iter().map(|l| l.id).collect::<Vec<_>>();
    assert_eq!(
        ids(list_links(db, LinkScope::User(a), None, None, 10)
            .await
            .unwrap()),
        [l2.id, l1.id]
    );
    assert_eq!(
        ids(list_links(db, LinkScope::All, None, None, 10)
            .await
            .unwrap()),
        [l3.id, l2.id, l1.id]
    );
    assert_eq!(
        ids(list_links(db, LinkScope::All, None, Some(l3.id), 10)
            .await
            .unwrap()),
        [l2.id, l1.id]
    );
    assert_eq!(
        ids(list_links(db, LinkScope::All, None, None, 1).await.unwrap()),
        [l3.id]
    );
    assert_eq!(
        ids(list_links(db, LinkScope::User(a), Some("seco"), None, 10)
            .await
            .unwrap()),
        [l2.id]
    );
    assert_eq!(
        ids(list_links(db, LinkScope::All, Some("three.com"), None, 10)
            .await
            .unwrap()),
        [l3.id]
    );
    assert!(
        list_links(db, LinkScope::User(b), Some("one"), None, 10)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn update_and_delete() {
    let app = test_app(&[]).await;
    let (db, cfg) = (&app.state.db, &app.state.config);
    let u = user(insert_user(db, "a@x.com", "user").await);
    let l = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", Some("old-slug"), false),
    )
    .await
    .unwrap();
    create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", Some("taken"), false),
    )
    .await
    .unwrap();
    let (nl, old) = update_link(
        db,
        l.id,
        link_edit(
            &app.state.config,
            Some("https://other.com"),
            Some("new-slug"),
            None,
        ),
    )
    .await
    .unwrap();
    assert_eq!((old.as_str(), nl.slug.as_str()), ("old-slug", "new-slug"));
    assert_eq!(nl.url, "https://other.com/");
    let c = update_link(
        db,
        l.id,
        link_edit(&app.state.config, None, Some("taken"), None),
    )
    .await;
    assert!(matches!(c, Err(AppError::Conflict(_))));
    assert!(matches!(
        update_link(db, 9999, link_edit(&app.state.config, None, None, None)).await,
        Err(AppError::NotFound)
    ));
    assert_eq!(
        delete_link(db, l.id).await.unwrap().as_deref(),
        Some("new-slug")
    );
    assert_eq!(delete_link(db, l.id).await.unwrap(), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn query_wildcards_are_literal() {
    let app = test_app(&[]).await;
    let (db, cfg) = (&app.state.db, &app.state.config);
    let u = user(insert_user(db, "a@x.com", "user").await);
    let pct = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://a.com/100%25", Some("pct"), false),
    )
    .await
    .unwrap();
    create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://a.com/1000", Some("zero"), false),
    )
    .await
    .unwrap();
    let us = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://b.com", Some("a_b"), false),
    )
    .await
    .unwrap();
    create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://b.com", Some("axb"), false),
    )
    .await
    .unwrap();
    let ids = |v: Vec<tiiny::links::model::Link>| v.into_iter().map(|l| l.id).collect::<Vec<_>>();
    assert_eq!(
        ids(list_links(db, LinkScope::All, Some("100%"), None, 10)
            .await
            .unwrap()),
        [pct.id]
    );
    assert_eq!(
        ids(list_links(db, LinkScope::All, Some("a_b"), None, 10)
            .await
            .unwrap()),
        [us.id]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn permanent_links() {
    let app = test_app(&[]).await;
    let (db, cfg) = (&app.state.db, &app.state.config);
    let u = user(insert_user(db, "a@x.com", "user").await);
    let p = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", Some("perm"), true),
    )
    .await
    .unwrap();
    assert!(p.permanent);
    for (url, slug) in [(Some("https://o.com"), None), (None, Some("other"))] {
        assert!(matches!(
            update_link(db, p.id, link_edit(&app.state.config, url, slug, None)).await,
            Err(AppError::Conflict(_))
        ));
    }
    assert!(matches!(
        update_link(
            db,
            p.id,
            link_edit(&app.state.config, None, None, Some(false))
        )
        .await,
        Err(AppError::Conflict(_))
    ));
    assert!(
        update_link(
            db,
            p.id,
            link_edit(&app.state.config, None, None, Some(true))
        )
        .await
        .unwrap()
        .0
        .permanent
    );

    let t = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", Some("temp"), false),
    )
    .await
    .unwrap();
    assert!(
        !update_link(
            db,
            t.id,
            link_edit(&app.state.config, None, None, Some(false))
        )
        .await
        .unwrap()
        .0
        .permanent
    );
    assert!(
        update_link(
            db,
            t.id,
            link_edit(&app.state.config, None, None, Some(true))
        )
        .await
        .unwrap()
        .0
        .permanent
    );

    let retired = |slug: &'static str| async move {
        let mut r = db
            .conn()
            .await
            .unwrap()
            .query("SELECT 1 FROM retired_slugs WHERE slug = ?1", [slug])
            .await
            .unwrap();
        r.next().await.unwrap().is_some()
    };
    delete_link(db, p.id).await.unwrap();
    assert!(retired("perm").await);
    let c = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", Some("perm"), false),
    )
    .await;
    assert!(matches!(c, Err(AppError::Conflict(m)) if m == "slug is already taken. Try another"));
    let x = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", Some("xyz"), false),
    )
    .await
    .unwrap();
    assert!(matches!(
        update_link(
            db,
            x.id,
            link_edit(&app.state.config, None, Some("perm"), None)
        )
        .await,
        Err(AppError::Conflict(_))
    ));

    let tmp = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", Some("tmp1"), false),
    )
    .await
    .unwrap();
    delete_link(db, tmp.id).await.unwrap();
    assert!(!retired("tmp1").await);
    create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", Some("tmp1"), false),
    )
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn create_never_returns_retired_slug() {
    let app = test_app(&[]).await;
    let (db, cfg) = (&app.state.db, &app.state.config);
    let u = user(insert_user(db, "a@x.com", "user").await);
    common::exec(
        db,
        "INSERT INTO retired_slugs (slug, retired_at) VALUES ('retired', 0)",
        (),
    )
    .await;
    // Random and custom slugs go through the same guarded insert.
    let c = create_link(
        db,
        cfg,
        &u,
        new_link(cfg, "https://example.com", Some("retired"), false),
    )
    .await;
    assert!(matches!(c, Err(AppError::Conflict(m)) if m == "slug is already taken. Try another"));
    for _ in 0..50 {
        let l = create_link(
            db,
            cfg,
            &u,
            new_link(cfg, "https://example.com", None, false),
        )
        .await
        .unwrap();
        assert_ne!(l.slug, "retired");
    }
}

const GUEST_INSERT: &str = "INSERT INTO links (user_id, guest_key_hash, slug, url, permanent_at, created_at, updated_at) VALUES (?1, ?2, 's', 'https://e.com', ?3, 0, 0)";

#[tokio::test(flavor = "multi_thread")]
async fn guest_link_has_no_owner() {
    let app = test_app(&[]).await;
    let db = &app.state.db;
    common::exec(
        db,
        GUEST_INSERT,
        libsql::params![None::<i64>, "h", None::<i64>],
    )
    .await;
    let id = common::scalar(db, "SELECT id FROM links", ()).await;
    let l = find_link(db, id).await.unwrap().unwrap();
    assert_eq!(l.user_id, None);
    assert!(!l.blocked);
}

#[tokio::test(flavor = "multi_thread")]
async fn owner_xor_guest_key_enforced() {
    let app = test_app(&[]).await;
    let db = &app.state.db;
    let uid = insert_user(db, "a@x.com", "user").await;
    let conn = db.conn().await.unwrap();
    for (u, g) in [(None, None), (Some(uid), Some("h"))] {
        assert!(
            conn.execute(GUEST_INSERT, libsql::params![u, g, None::<i64>])
                .await
                .is_err()
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn guest_link_cannot_be_permanent() {
    let app = test_app(&[]).await;
    let conn = app.state.db.conn().await.unwrap();
    assert!(
        conn.execute(GUEST_INSERT, libsql::params![None::<i64>, "h", 1])
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn update_link_refusals_name_the_reason() {
    let app = test_app(&[]).await;
    let (db, cfg) = (&app.state.db, &app.state.config);
    let u = user(insert_user(db, "a@x.com", "user").await);
    let link = |slug: &'static str, permanent| {
        create_link(
            db,
            cfg,
            &u,
            new_link(cfg, "https://example.com", Some(slug), permanent),
        )
    };
    let p = link("perm", true).await.unwrap();
    let t = link("temp", false).await.unwrap();
    link("taken", false).await.unwrap();
    let gone = link("gone", true).await.unwrap();
    delete_link(db, gone.id).await.unwrap();
    let msg = |r: Result<(tiiny::links::model::Link, String), AppError>| match r {
        Err(AppError::Conflict(m) | AppError::Unprocessable(m)) => m,
        other => panic!("expected a refusal, got {:?}", other.map(|(l, _)| l.slug)),
    };
    let edit = |id, url, slug, perm| update_link(db, id, link_edit(cfg, url, slug, perm));
    assert_eq!(
        msg(edit(p.id, Some("https://o.com"), None, None).await),
        "permanent links cannot be edited"
    );
    assert_eq!(
        msg(edit(p.id, None, Some("other"), None).await),
        "permanent links cannot be edited"
    );
    assert_eq!(
        msg(edit(p.id, None, None, Some(false)).await),
        "permanent links cannot be made temporary"
    );
    assert_eq!(
        msg(edit(t.id, None, Some("taken"), None).await),
        "slug is already taken. Try another"
    );
    assert_eq!(
        msg(edit(t.id, None, Some("gone"), None).await),
        "slug is already taken. Try another"
    );
    // Untouched by the refusals; a no-op edit and making it temporary-already-temporary still succeed.
    let (same, old) = edit(t.id, None, None, Some(false)).await.unwrap();
    assert_eq!(
        (same.slug.as_str(), old.as_str(), same.url.as_str()),
        ("temp", "temp", "https://example.com/")
    );
    let (moved, old) = edit(t.id, Some("https://n.com"), Some("temp"), None)
        .await
        .unwrap();
    assert_eq!(
        (moved.slug.as_str(), old.as_str(), moved.url.as_str()),
        ("temp", "temp", "https://n.com/")
    );
    let (made, _) = edit(t.id, None, None, Some(true)).await.unwrap();
    assert!(made.permanent);
    assert_eq!(
        msg(edit(made.id, None, None, Some(false)).await),
        "permanent links cannot be made temporary"
    );
}
