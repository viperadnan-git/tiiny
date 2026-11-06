mod common;

use common::{exec, insert_user, scalar, test_app};
use tiiny::db::{Db, now_unix, purge_stale, today_utc_day};

async fn try_exec(db: &Db, sql: &str) -> Result<u64, anyhow::Error> {
    Ok(db.conn().await?.execute(sql, ()).await?)
}

#[tokio::test]
async fn migrate_creates_tables() {
    let app = test_app(&[]).await;
    let conn = app.state.db.conn().await.unwrap();
    let mut rows = conn
        .query("SELECT name FROM sqlite_master WHERE type='table'", ())
        .await
        .unwrap();
    let mut names = vec![];
    while let Some(r) = rows.next().await.unwrap() {
        names.push(r.get::<String>(0).unwrap());
    }
    for t in [
        "users",
        "oauth_accounts",
        "sessions",
        "email_tokens",
        "links",
        "link_quota",
        "abuse_reports",
        "link_stats",
    ] {
        assert!(names.contains(&t.to_string()), "missing {t}");
    }
}

#[tokio::test]
async fn migrate_is_idempotent() {
    let app = test_app(&[]).await;
    app.state.db.migrate().await.unwrap();
    app.state.db.migrate().await.unwrap();
}

#[tokio::test]
async fn foreign_keys_enforced() {
    let app = test_app(&[]).await;
    let r = try_exec(
        &app.state.db,
        "INSERT INTO links (user_id, slug, url, created_at, updated_at) VALUES (999, 'abc', 'http://x', 0, 0)",
    )
    .await;
    assert!(r.is_err());
}

#[tokio::test]
async fn cascade_delete() {
    let app = test_app(&[]).await;
    let db = &app.state.db;
    seed(db).await;
    exec(
        db,
        "INSERT INTO link_stats (link_id, epoch_day, dimension, dimension_value, click_count) VALUES (1, 0, 0, '', 1)",
        (),
    )
    .await;
    exec(db, "DELETE FROM users WHERE id = 1", ()).await;
    assert_eq!(scalar(db, "SELECT COUNT(*) FROM links", ()).await, 0);
    assert_eq!(scalar(db, "SELECT COUNT(*) FROM link_stats", ()).await, 0);
}

async fn seed(db: &Db) {
    exec(
        db,
        "INSERT INTO users (id, email, created_at) VALUES (1, 'a@b.c', 0)",
        (),
    )
    .await;
    exec(db, "INSERT INTO links (id, user_id, slug, url, created_at, updated_at) VALUES (1, 1, 'abc', 'http://x', 0, 0)", ()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn strict_rejects_wrong_type() {
    let app = test_app(&[]).await;
    seed(&app.state.db).await;
    assert!(
        try_exec(
            &app.state.db,
            "UPDATE links SET click_count = 'abc' WHERE id = 1"
        )
        .await
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn check_constraints_enforced() {
    let app = test_app(&[]).await;
    seed(&app.state.db).await;
    for sql in [
        "INSERT INTO users (email, role, created_at) VALUES ('r@x.com', 'root', 0)",
        "INSERT INTO users (email, created_at) VALUES ('A@x.com', 0)",
        "INSERT INTO oauth_accounts (user_id, provider, provider_user_id, created_at) VALUES (1, 'gitlab', 'p', 0)",
        "INSERT INTO email_tokens (user_id, purpose, token_hash, expires_at) VALUES (1, 'other', 'h', 0)",
        "INSERT INTO link_stats (link_id, epoch_day, dimension, dimension_value, click_count) VALUES (1, 0, 6, '', 1)",
        "INSERT INTO link_stats (link_id, epoch_day, dimension, dimension_value, click_count) VALUES (1, 0, 0, '', -1)",
        "UPDATE links SET click_count = -1 WHERE id = 1",
    ] {
        assert!(
            try_exec(&app.state.db, sql).await.is_err(),
            "accepted: {sql}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_write_transactions_succeed() {
    let app = test_app(&[]).await;
    let tasks: Vec<_> = (0..20)
        .map(|i| {
            let db = app.state.db.clone();
            tokio::spawn(async move {
                async {
                    let w = &db.conn().await?;
                    let tx = w.transaction().await?;
                    tx.execute(
                        "INSERT INTO users (email, created_at) VALUES (?1, 0)",
                        [format!("u{i}@x.com")],
                    )
                    .await?;
                    tx.commit().await?;
                    Ok::<_, anyhow::Error>(())
                }
                .await
                .unwrap();
            })
        })
        .collect();
    for t in tasks {
        t.await.unwrap();
    }
    assert_eq!(
        scalar(&app.state.db, "SELECT COUNT(*) FROM users", ()).await,
        20
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn local_db_uses_wal() {
    let app = test_app(&[]).await;
    let mode: String = common::first_row(&app.state.db, "PRAGMA journal_mode", ())
        .await
        .get(0)
        .unwrap();
    assert_eq!(mode, "wal");
}

#[tokio::test]
async fn open_creates_missing_data_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("a/b");
    let config = |d: &std::path::Path| common::config(d, &[]);
    tiiny::db::Db::open(&config(&dir)).await.unwrap();
    assert!(dir.join("tiiny.db").exists());
    let other = tmp.path().join("c");
    tiiny::db::Db::open_direct(&config(&other)).await.unwrap();
    assert!(
        other.join("tiiny.db").exists(),
        "local mode: CLI opens the local file"
    );

    let blocker = tmp.path().join("file");
    std::fs::write(&blocker, "").unwrap();
    let err = tiiny::db::Db::open(&config(&blocker.join("sub")))
        .await
        .err()
        .expect("open must fail when the dir can't be created")
        .to_string();
    assert!(err.contains("creating data dir"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn purge_removes_spent_tokens_and_old_daily_counts() {
    let app = test_app(&[]).await;
    let db = &app.state.db;
    let uid = insert_user(db, "a@x.com", "user").await;
    let now = now_unix();
    for (hash, expires_at, used_at) in [
        ("live", now + 60, None),
        ("used", now + 60, Some(now)),
        ("expired", now - 1, None),
    ] {
        exec(
            db,
            "INSERT INTO email_tokens (user_id, purpose, token_hash, expires_at, used_at) VALUES (?1, 'verify_email', ?2, ?3, ?4)",
            libsql::params![uid, hash, expires_at, used_at],
        )
        .await;
    }
    let today = today_utc_day();
    for day in [today, today - 1, today - 2] {
        exec(
            db,
            "INSERT INTO link_quota (owner, epoch_day, created) VALUES ('u:' || ?1, ?2, 1)",
            [uid, day],
        )
        .await;
    }
    for day in [today, today - 1, today - 2] {
        exec(
            db,
            "INSERT INTO link_quota (owner, epoch_day, created) VALUES ('ip:1.2.3.4', ?1, 1)",
            [day],
        )
        .await;
    }
    purge_stale(db).await.unwrap();
    let count = |sql: &'static str| scalar(db, sql, ());
    assert_eq!(
        count("SELECT COUNT(*) FROM email_tokens WHERE token_hash = 'live'").await,
        1
    );
    assert_eq!(count("SELECT COUNT(*) FROM email_tokens").await, 1);
    assert_eq!(
        scalar(
            db,
            "SELECT COUNT(*) FROM link_quota WHERE owner LIKE 'u:%' AND epoch_day >= ?1",
            [today - 1]
        )
        .await,
        2
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM link_quota WHERE owner LIKE 'u:%'").await,
        2
    );
    assert_eq!(
        scalar(
            db,
            "SELECT COUNT(*) FROM link_quota WHERE owner LIKE 'ip:%' AND epoch_day >= ?1",
            [today - 1]
        )
        .await,
        2
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM link_quota WHERE owner LIKE 'ip:%'").await,
        2
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_user_clears_their_link_quota() {
    let app = test_app(&[]).await;
    let db = &app.state.db;
    let uid = insert_user(db, "q@x.com", "user").await;
    let other = insert_user(db, "r@x.com", "user").await;
    for id in [uid, other] {
        exec(
            db,
            "INSERT INTO link_quota (owner, epoch_day, created) VALUES ('u:' || ?1, ?2, 3)",
            [id, today_utc_day()],
        )
        .await;
    }
    tiiny::auth::delete_user(db, uid).await.unwrap();
    assert_eq!(
        scalar(
            db,
            "SELECT COUNT(*) FROM link_quota WHERE owner = 'u:' || ?1",
            [uid]
        )
        .await,
        0
    );
    assert_eq!(scalar(db, "SELECT COUNT(*) FROM link_quota", ()).await, 1);
}
