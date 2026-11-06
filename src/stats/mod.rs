pub mod classify;
pub mod collector;

use std::str::FromStr;

use serde::Serialize;

use crate::{
    db::{Db, today_utc_day},
    error::AppError,
};

pub const MAX_REFERRERS_PER_DAY: usize = 100;
pub const OTHER_REFERRER: &str = "other";

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Total = 0,
    Referrer = 1,
    Country = 2,
    Device = 3,
    Browser = 4,
    Os = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsRange {
    Days7,
    Days30,
    Days90,
    All,
}

impl FromStr for StatsRange {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, AppError> {
        match s {
            "7d" => Ok(Self::Days7),
            "30d" => Ok(Self::Days30),
            "90d" => Ok(Self::Days90),
            "all" => Ok(Self::All),
            _ => Err(AppError::BadRequest(
                "range must be 7d, 30d, 90d or all".into(),
            )),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DayCount {
    pub day: i64,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct ValueCount {
    pub value: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct LinkStats {
    pub click_count: i64,
    pub series: Vec<DayCount>,
    pub referrers: Vec<ValueCount>,
    pub countries: Vec<ValueCount>,
    pub devices: Vec<ValueCount>,
    pub browsers: Vec<ValueCount>,
    pub oses: Vec<ValueCount>,
}

const TOP_N: usize = 10;

pub async fn link_stats(db: &Db, link_id: i64, range: StatsRange) -> Result<LinkStats, AppError> {
    let conn = db.conn().await?;
    let today = today_utc_day();
    let mut rows = conn
        .query(
            "SELECT click_count, \
             (SELECT MIN(epoch_day) FROM link_stats WHERE link_id = ?1 AND dimension = 0) \
             FROM links WHERE id = ?1",
            [link_id],
        )
        .await?;
    let row = rows.next().await?.ok_or(AppError::NotFound)?;
    let click_count: i64 = row.get(0)?;
    let start = match range {
        StatsRange::Days7 => today - 6,
        StatsRange::Days30 => today - 29,
        StatsRange::Days90 => today - 89,
        StatsRange::All => row.get::<Option<i64>>(1)?.unwrap_or(today).min(today),
    };

    let mut stats = LinkStats {
        click_count,
        series: (start..=today)
            .map(|day| DayCount { day, count: 0 })
            .collect(),
        referrers: vec![],
        countries: vec![],
        devices: vec![],
        browsers: vec![],
        oses: vec![],
    };
    let mut rows = conn
        .query(
            "SELECT epoch_day, click_count FROM link_stats \
             WHERE link_id = ?1 AND dimension = 0 AND epoch_day >= ?2",
            [link_id, start],
        )
        .await?;
    while let Some(r) = rows.next().await? {
        let (day, count): (i64, i64) = (r.get(0)?, r.get(1)?);
        if let Some(p) = stats.series.get_mut((day - start) as usize) {
            p.count = count;
        }
    }
    let mut rows = conn
        .query(
            "SELECT dimension, dimension_value, SUM(click_count) AS n FROM link_stats \
             WHERE link_id = ?1 AND dimension != 0 AND epoch_day >= ?2 \
             GROUP BY dimension, dimension_value ORDER BY n DESC, dimension_value",
            [link_id, start],
        )
        .await?;
    while let Some(r) = rows.next().await? {
        let list = match r.get::<u32>(0)? {
            1 => &mut stats.referrers,
            2 => &mut stats.countries,
            3 => &mut stats.devices,
            4 => &mut stats.browsers,
            5 => &mut stats.oses,
            _ => continue,
        };
        if list.len() < TOP_N {
            list.push(ValueCount {
                value: r.get(1)?,
                count: r.get(2)?,
            });
        }
    }
    Ok(stats)
}
