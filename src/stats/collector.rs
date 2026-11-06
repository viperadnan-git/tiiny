use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    sync::{Arc, Mutex},
};

use axum::http::HeaderMap;

use super::{Dimension, MAX_REFERRERS_PER_DAY, OTHER_REFERRER, classify::Classifier};
use crate::{
    config::Config,
    db::{Db, today_utc_day},
};

type Key = (i64, i64, u8, String);

#[derive(Default)]
struct Batch {
    stats: HashMap<Key, i64>,
    clicks: HashMap<i64, i64>,
    referrers: HashMap<(i64, i64), usize>,
}

impl Batch {
    fn add(&mut self, mut key: Key, n: i64) {
        if key.2 == Dimension::Referrer as u8
            && key.3 != OTHER_REFERRER
            && !self.stats.contains_key(&key)
        {
            let distinct = self.referrers.entry((key.0, key.1)).or_default();
            if *distinct >= MAX_REFERRERS_PER_DAY {
                key.3 = OTHER_REFERRER.into();
            } else {
                *distinct += 1;
            }
        }
        *self.stats.entry(key).or_default() += n;
    }

    fn merge(&mut self, other: Batch) {
        for (k, n) in other.stats {
            self.add(k, n);
        }
        for (k, n) in other.clicks {
            *self.clicks.entry(k).or_default() += n;
        }
    }
}

#[derive(Clone)]
pub struct StatsCollector {
    classifier: Arc<Classifier>,
    pending: Arc<Mutex<Batch>>,
}

impl StatsCollector {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        Ok(Self {
            classifier: Arc::new(Classifier::new(config)?),
            pending: Default::default(),
        })
    }

    pub fn record(&self, link_id: i64, headers: &HeaderMap, client_ip: Option<IpAddr>) {
        let Some(info) = self.classifier.classify(headers, client_ip) else {
            return;
        };
        let day = today_utc_day();
        let mut b = self.pending.lock().unwrap();
        for (dim, value) in [
            (Dimension::Total, String::new()),
            (Dimension::Referrer, info.referrer),
            (Dimension::Country, info.country),
            (Dimension::Device, info.device.into()),
            (Dimension::Browser, info.browser),
            (Dimension::Os, info.os),
        ] {
            b.add((link_id, day, dim as u8, value), 1);
        }
        *b.clicks.entry(link_id).or_default() += 1;
    }

    pub async fn flush(&self, db: &Db) -> anyhow::Result<()> {
        let batch = std::mem::take(&mut *self.pending.lock().unwrap());
        if batch.clicks.is_empty() {
            return Ok(());
        }
        let started = std::time::Instant::now();
        if let Err(e) = write(db, &batch).await {
            self.pending.lock().unwrap().merge(batch);
            return Err(e);
        }
        if let Err(e) = db.catch_up().await {
            tracing::warn!(error = ?e, "replica catch-up failed");
        }
        tracing::debug!(
            links = batch.clicks.len(),
            clicks = batch.clicks.values().sum::<i64>(),
            latency_ms = started.elapsed().as_millis() as u64,
            "stats flushed"
        );
        Ok(())
    }
}

const CHUNK: usize = 500;

fn placeholders(rows: usize, cols: usize) -> String {
    let row = format!("({})", vec!["?"; cols].join(","));
    vec![row; rows].join(",")
}

async fn write(db: &Db, b: &Batch) -> anyhow::Result<()> {
    let conn = db.conn().await?;
    let tx = conn
        .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
        .await?;

    let pairs: Vec<(i64, i64)> = b
        .stats
        .keys()
        .filter(|k| k.2 == Dimension::Referrer as u8)
        .map(|k| (k.0, k.1))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let mut existing: HashMap<(i64, i64), HashSet<String>> = HashMap::new();
    for chunk in pairs.chunks(CHUNK) {
        let mut params: Vec<libsql::Value> = vec![OTHER_REFERRER.into()];
        params.extend(chunk.iter().flat_map(|&(l, d)| [l.into(), d.into()]));
        let mut rows = tx
            .query(
                &format!(
                    "SELECT link_id, epoch_day, dimension_value FROM link_stats \
                     WHERE dimension = 1 AND dimension_value != ? \
                     AND (link_id, epoch_day) IN (VALUES {})",
                    placeholders(chunk.len(), 2)
                ),
                params,
            )
            .await?;
        while let Some(r) = rows.next().await? {
            existing
                .entry((r.get(0)?, r.get(1)?))
                .or_default()
                .insert(r.get(2)?);
        }
    }

    // New referrer hosts beyond the per-link/day cap fold into OTHER_REFERRER.
    let mut added: HashMap<(i64, i64), usize> = HashMap::new();
    let mut stats: HashMap<Key, i64> = HashMap::new();
    for (key, n) in &b.stats {
        let mut key = key.clone();
        if key.2 == Dimension::Referrer as u8 && key.3 != OTHER_REFERRER {
            let seen = existing.get(&(key.0, key.1));
            if !seen.is_some_and(|s| s.contains(&key.3)) {
                let new = added.entry((key.0, key.1)).or_default();
                if seen.map_or(0, |s| s.len()) + *new >= MAX_REFERRERS_PER_DAY {
                    key.3 = OTHER_REFERRER.into();
                } else {
                    *new += 1;
                }
            }
        }
        *stats.entry(key).or_default() += n;
    }

    let stats: Vec<_> = stats.into_iter().collect();
    for chunk in stats.chunks(CHUNK) {
        let params: Vec<libsql::Value> = chunk
            .iter()
            .flat_map(|((link_id, day, dim, value), n)| {
                [
                    (*link_id).into(),
                    (*day).into(),
                    (*dim as i64).into(),
                    value.clone().into(),
                    (*n).into(),
                ]
            })
            .collect();
        // WHERE EXISTS: a link deleted since the click must not abort the batch with an FK error.
        tx.execute(
            &format!(
                "INSERT INTO link_stats (link_id, epoch_day, dimension, dimension_value, click_count) \
                 SELECT column1, column2, column3, column4, column5 FROM (VALUES {}) \
                 WHERE EXISTS (SELECT 1 FROM links WHERE id = column1) \
                 ON CONFLICT (link_id, epoch_day, dimension, dimension_value) \
                 DO UPDATE SET click_count = click_count + excluded.click_count",
                placeholders(chunk.len(), 5)
            ),
            params,
        )
        .await?;
    }

    let clicks: Vec<_> = b.clicks.iter().collect();
    for chunk in clicks.chunks(CHUNK) {
        let mut params: Vec<libsql::Value> = chunk
            .iter()
            .flat_map(|(id, n)| [(**id).into(), (**n).into()])
            .collect();
        params.extend(chunk.iter().map(|(id, _)| libsql::Value::from(**id)));
        tx.execute(
            &format!(
                "UPDATE links SET click_count = click_count + CASE id {} END WHERE id IN ({})",
                "WHEN ? THEN ? ".repeat(chunk.len()),
                vec!["?"; chunk.len()].join(",")
            ),
            params,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
