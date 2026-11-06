mod common;

use axum::http::StatusCode;
use common::{TestApp, add_link, insert_user, new_link, scalar, test_app};
use tiiny::{
    db::{Db, prune_stats, today_utc_day},
    links::model::{UserRef, create_link, delete_link},
    stats::{Dimension, StatsRange, link_stats},
};

const CHROME: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

async fn setup() -> (TestApp, i64) {
    // CF-IPCountry (sent by `click`) is only read behind Cloudflare.
    let app = test_app(&[("TIINY_CLIENT_IP_HEADER", "cf-connecting-ip")]).await;
    let uid = insert_user(&app.state.db, "a@x.com", "user").await;
    let l = add_link(&app, uid, "abc", "https://example.com/", false).await;
    (app, l.id)
}

async fn click(app: &TestApp, slug: &str, ua: &str) {
    app.server
        .get(&format!("/{slug}"))
        .add_header("user-agent", ua)
        .add_header("referer", "https://news.ycombinator.com/item?id=1")
        .add_header("cf-ipcountry", "DE")
        .await
        .assert_status(StatusCode::FOUND);
}

async fn link_clicks(db: &Db, id: i64) -> i64 {
    scalar(db, "SELECT click_count FROM links WHERE id = ?1", [id]).await
}

async fn stat(db: &Db, id: i64, dim: Dimension, value: &str) -> i64 {
    scalar(
        db,
        "SELECT COALESCE(SUM(click_count), 0) FROM link_stats \
         WHERE link_id = ?1 AND epoch_day = ?2 AND dimension = ?3 AND dimension_value = ?4",
        libsql::params![id, today_utc_day(), dim as u8, value],
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn flush_persists_and_accumulates() {
    let (app, id) = setup().await;
    let db = &app.state.db;
    for _ in 0..3 {
        click(&app, "abc", CHROME).await;
    }
    app.state.stats.flush(db).await.unwrap();
    assert_eq!(link_clicks(db, id).await, 3);
    assert_eq!(stat(db, id, Dimension::Total, "").await, 3);
    assert_eq!(
        stat(db, id, Dimension::Referrer, "news.ycombinator.com").await,
        3
    );
    assert_eq!(stat(db, id, Dimension::Country, "DE").await, 3);
    assert_eq!(stat(db, id, Dimension::Device, "desktop").await, 3);
    assert_eq!(stat(db, id, Dimension::Browser, "Chrome").await, 3);
    assert_eq!(stat(db, id, Dimension::Os, "Windows").await, 3);

    click(&app, "abc", CHROME).await;
    app.state.stats.flush(db).await.unwrap();
    app.state.stats.flush(db).await.unwrap();
    assert_eq!(link_clicks(db, id).await, 4);
    assert_eq!(stat(db, id, Dimension::Total, "").await, 4);
}

#[tokio::test(flavor = "multi_thread")]
async fn bots_redirect_but_not_counted() {
    let (app, id) = setup().await;
    click(&app, "abc", "Slackbot-LinkExpanding 1.0").await;
    app.state.stats.flush(&app.state.db).await.unwrap();
    assert_eq!(link_clicks(&app.state.db, id).await, 0);
    assert_eq!(
        scalar(&app.state.db, "SELECT COUNT(*) FROM link_stats", ()).await,
        0
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_link_does_not_abort_flush() {
    let (app, a) = setup().await;
    let db = &app.state.db;
    let u = UserRef {
        id: 1,
        is_admin: false,
    };
    let b = create_link(
        db,
        &app.state.config,
        &u,
        new_link(&app.state.config, "https://b.com/", Some("bbb")),
    )
    .await
    .unwrap()
    .id;
    click(&app, "abc", CHROME).await;
    click(&app, "bbb", CHROME).await;
    delete_link(db, b).await.unwrap();
    app.state.stats.flush(db).await.unwrap();
    assert_eq!(link_clicks(db, a).await, 1);
    assert_eq!(stat(db, a, Dimension::Total, "").await, 1);
    assert_eq!(
        scalar(
            db,
            "SELECT COUNT(*) FROM link_stats WHERE link_id = ?1",
            [b]
        )
        .await,
        0
    );
}

async fn insert_stat(db: &Db, id: i64, day: i64, dim: Dimension, value: &str, n: i64) {
    common::exec(
        db,
        "INSERT INTO link_stats VALUES (?1, ?2, ?3, ?4, ?5)",
        libsql::params![id, day, dim as u8, value, n],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn prune_removes_old_rows_only() {
    let (app, id) = setup().await;
    let db = &app.state.db;
    let today = today_utc_day();
    common::exec(db, "UPDATE links SET click_count = 9 WHERE id = ?1", [id]).await;
    insert_stat(db, id, today - 31, Dimension::Total, "", 5).await;
    insert_stat(db, id, today - 31, Dimension::Country, "DE", 5).await;
    insert_stat(db, id, today - 30, Dimension::Total, "", 2).await;
    insert_stat(db, id, today, Dimension::Total, "", 2).await;
    assert_eq!(prune_stats(db, 30).await.unwrap(), 2);
    assert_eq!(
        scalar(db, "SELECT MIN(epoch_day) FROM link_stats", ()).await,
        today - 30
    );
    assert_eq!(link_clicks(db, id).await, 9);
}

#[tokio::test(flavor = "multi_thread")]
async fn stats_query() {
    let (app, id) = setup().await;
    let db = &app.state.db;
    let today = today_utc_day();
    insert_stat(db, id, today - 40, Dimension::Total, "", 1).await;
    insert_stat(db, id, today - 2, Dimension::Total, "", 3).await;
    insert_stat(db, id, today, Dimension::Total, "", 4).await;
    insert_stat(db, id, today - 2, Dimension::Country, "DE", 3).await;
    insert_stat(db, id, today, Dimension::Country, "DE", 1).await;
    insert_stat(db, id, today, Dimension::Country, "IN", 3).await;
    insert_stat(db, id, today - 40, Dimension::Country, "US", 9).await;
    for i in 0..12 {
        insert_stat(
            db,
            id,
            today,
            Dimension::Referrer,
            &format!("r{i:02}"),
            i + 1,
        )
        .await;
    }

    let s = link_stats(db, id, "7d".parse().unwrap()).await.unwrap();
    let v = serde_json::to_value(&s).unwrap();
    let series = v["series"].as_array().unwrap();
    assert_eq!(series.len(), 7);
    assert_eq!(series[0]["day"], today - 6);
    assert_eq!(series[6]["day"], today);
    let counts: Vec<i64> = series
        .iter()
        .map(|p| p["count"].as_i64().unwrap())
        .collect();
    assert_eq!(counts, [0, 0, 0, 0, 3, 0, 4]);
    assert_eq!(
        v["countries"],
        serde_json::json!([{"value": "DE", "count": 4}, {"value": "IN", "count": 3}])
    );
    let refs = v["referrers"].as_array().unwrap();
    assert_eq!(refs.len(), 10);
    assert_eq!(refs[0]["value"], "r11");
    assert_eq!(v["devices"], serde_json::json!([]));
    assert_eq!(v["click_count"], 0);

    let s = link_stats(db, id, StatsRange::All).await.unwrap();
    assert_eq!(s.series.len(), 41);
    assert_eq!(s.series[0].day, today - 40);
    assert_eq!(s.countries[0].value, "US");

    for r in ["30d", "90d", "all"] {
        assert!(r.parse::<StatsRange>().is_ok());
    }
    assert!("1y".parse::<StatsRange>().is_err());
    assert_eq!(
        link_stats(db, id, StatsRange::Days30)
            .await
            .unwrap()
            .series
            .len(),
        30
    );

    let fresh = create_link(
        db,
        &app.state.config,
        &UserRef {
            id: 1,
            is_admin: false,
        },
        new_link(&app.state.config, "https://f.com/", Some("fresh")),
    )
    .await
    .unwrap();
    let s = link_stats(db, fresh.id, StatsRange::All).await.unwrap();
    assert_eq!(s.series.len(), 1);
    assert_eq!(s.series[0].day, today);
}

async fn referrer_counts(db: &Db, id: i64) -> (i64, i64, i64) {
    let distinct = scalar(
        db,
        "SELECT COUNT(*) FROM link_stats WHERE link_id = ?1 AND dimension = 1 AND dimension_value != 'other'",
        [id],
    )
    .await;
    (
        distinct,
        stat(db, id, Dimension::Referrer, "other").await,
        stat(db, id, Dimension::Total, "").await,
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn referrers_capped_per_link_day() {
    let (app, a) = setup().await;
    let db = &app.state.db;
    let u = UserRef {
        id: 1,
        is_admin: false,
    };
    let b = create_link(
        db,
        &app.state.config,
        &u,
        new_link(&app.state.config, "https://b.com/", Some("bbb")),
    )
    .await
    .unwrap()
    .id;
    let rec = |id: i64, i: usize| {
        let mut h = axum::http::HeaderMap::new();
        h.insert("user-agent", CHROME.parse().unwrap());
        h.insert("referer", format!("https://h{i}.com/").parse().unwrap());
        app.state.stats.record(id, &h, None);
    };

    for i in 0..105 {
        rec(a, i);
    }
    app.state.stats.flush(db).await.unwrap();
    assert_eq!(referrer_counts(db, a).await, (100, 5, 105));

    for i in 0..60 {
        rec(b, i);
    }
    app.state.stats.flush(db).await.unwrap();
    for i in 60..105 {
        rec(b, i);
    }
    rec(b, 0);
    app.state.stats.flush(db).await.unwrap();
    assert_eq!(referrer_counts(db, b).await, (100, 5, 106));
    assert_eq!(stat(db, b, Dimension::Referrer, "h0.com").await, 2);
    assert_eq!(link_clicks(db, b).await, 106);
}

#[tokio::test(flavor = "multi_thread")]
async fn flush_handles_batches_beyond_one_chunk() {
    let (app, _) = setup().await;
    let db = &app.state.db;
    let ids: Vec<i64> = (1000..1600).collect();
    async {
        let w = &db.conn().await?;
        for id in &ids {
            w.execute(
                "INSERT INTO links (id, user_id, slug, url, created_at, updated_at) VALUES (?1, 1, ?2, 'https://x.com/', 0, 0)",
                libsql::params![*id, format!("s{id}")],
            )
            .await?;
        }
        Ok::<_, anyhow::Error>(())
    }
    .await
    .unwrap();
    let mut h = axum::http::HeaderMap::new();
    h.insert("user-agent", CHROME.parse().unwrap());
    h.insert("referer", "https://r.com/".parse().unwrap());
    for id in &ids {
        app.state.stats.record(*id, &h, None);
        app.state.stats.record(*id, &h, None);
    }
    app.state.stats.flush(db).await.unwrap();
    assert_eq!(
        scalar(
            db,
            "SELECT SUM(click_count) FROM links WHERE id >= 1000",
            ()
        )
        .await,
        1200
    );
    assert_eq!(stat(db, 1599, Dimension::Referrer, "r.com").await, 2);
    assert_eq!(
        scalar(
            db,
            "SELECT COUNT(*) FROM link_stats WHERE link_id >= 1000 AND dimension = 0",
            ()
        )
        .await,
        600
    );
}
