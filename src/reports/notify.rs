use std::time::Duration;

use crate::{
    AppState,
    auth::email::{Letter, Mail, escape},
    config::Config,
};

const LIST_MAX: i64 = 20;

async fn max_id(s: &AppState) -> anyhow::Result<i64> {
    let conn = s.db.conn().await?;
    let mut rows = conn
        .query("SELECT COALESCE(MAX(id), 0) FROM abuse_reports", ())
        .await?;
    Ok(rows.next().await?.map_or(Ok(0), |r| r.get(0))?)
}

/// Emails admins about reports with `id > after_id` that are still open (one acted on before the
/// email goes out is left out); returns the new cursor, past resolved ones too.
pub async fn notify_once(s: &AppState, after_id: i64) -> anyhow::Result<i64> {
    if !s.mailer.is_enabled() {
        return Ok(after_id);
    }
    let conn = s.db.conn().await?;
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FILTER (WHERE status = 'open'), COALESCE(MAX(id), ?1)
             FROM abuse_reports WHERE id > ?1",
            [after_id],
        )
        .await?;
    let r = rows.next().await?.expect("aggregate row");
    let (total, latest): (i64, i64) = (r.get(0)?, r.get(1)?);
    drop(rows);
    if total == 0 {
        return Ok(latest);
    }
    let mut rows = conn
        .query(
            "SELECT slug, url, reason FROM abuse_reports
             WHERE id > ?1 AND id <= ?2 AND status = 'open' ORDER BY id LIMIT ?3",
            libsql::params![after_id, latest, LIST_MAX],
        )
        .await?;
    let mut listed = vec![];
    while let Some(r) = rows.next().await? {
        listed.push((r.get(0)?, r.get(1)?, r.get(2)?));
    }
    drop(rows);
    let mut rows = conn
        .query(
            "SELECT email FROM users WHERE role = 'admin' AND disabled_at IS NULL ORDER BY id",
            (),
        )
        .await?;
    let mut admins = vec![];
    while let Some(r) = rows.next().await? {
        admins.push(r.get::<String>(0)?);
    }
    drop(rows);
    if admins.is_empty() {
        return Ok(latest);
    }
    let m = digest(&s.config, &listed, total);
    // One message, all admins in Bcc; most SMTP providers still count each recipient.
    if let Err(e) = s.mailer.send_bcc(&admins, &m.subject, m.body, m.html).await {
        tracing::error!(error = ?e, admins = admins.len(), "abuse report email failed");
    }
    Ok(latest)
}

pub fn spawn(s: AppState) {
    tokio::spawn(async move {
        let mut cursor = match max_id(&s).await {
            Ok(id) => id,
            Err(e) => {
                tracing::error!(error = ?e, "report notifier start failed");
                return;
            }
        };
        let mut tick = tokio::time::interval(s.config.report_email_interval);
        tick.tick().await;
        loop {
            tick.tick().await;
            match notify_once(&s, cursor).await {
                Ok(id) => cursor = id,
                Err(e) => tracing::error!(error = ?e, "report notifier failed"),
            }
        }
    });
}

/// The summary of `total` open reports, `listed` (slug, destination, reason) being the first [`LIST_MAX`].
pub fn digest(config: &Config, listed: &[(String, String, String)], total: i64) -> Mail {
    let origin = config.origin();
    let mut lines: Vec<String> = listed
        .iter()
        .map(|(slug, url, reason)| format!("{origin}/{slug} → {url} ({reason})"))
        .collect();
    let rows_html: String = listed
        .iter()
        .map(|(slug, url, reason)| report_row(slug, url, reason))
        .collect();
    let more = (total > LIST_MAX).then(|| format!("…and {} more", total - LIST_MAX));
    lines.extend(more.clone());
    let review = format!("{origin}/admin/reports");
    lines.push(format!("Review: {review}"));
    let rows_html = format!(
        r#"<tr><td class="pad" style="padding:22px 40px 0;">{rows_html}{}</td></tr>"#,
        more.map(|m| format!(
            r#"<div class="soft" style="padding-top:12px;font-family:'Public Sans',Arial,Helvetica,sans-serif;font-size:14px;color:#5d6b66;">{}</div>"#,
            escape(&m)
        ))
        .unwrap_or_default()
    );
    let every = spoken(config.report_email_interval);
    let intro = if total == 1 {
        format!(
            "Someone reported a link in the last {every}. Review it, and block or delete the link if it breaks your terms."
        )
    } else {
        format!(
            "People sent {total} reports in the last {every}. Review them, and block or delete any link that breaks your terms."
        )
    };
    Mail {
        subject: if total == 1 {
            "1 new abuse report".to_owned()
        } else {
            format!("{total} new abuse reports")
        },
        body: lines.join("\n"),
        html: Letter {
            to: "Admins of this tiiny server",
            stamp: (if total == 1 { "Report" } else { "Reports" }, &total.to_string()),
            title: if total == 1 { "New abuse report" } else { "New abuse reports" },
            intro: &intro,
            rows: rows_html,
            action: "Review reports",
            url: &review,
            note: &format!("You're getting this because you're an admin of this tiiny server. New reports are sent together, at most every {every}."),
        }
        .render(origin),
    }
}

/// "10 minutes", "1 hour 30 minutes"; a lone single unit drops its "1" ("hour"), so it reads
/// after "in the last" and "every".
fn spoken(d: Duration) -> String {
    let mut left = d.as_secs().max(1);
    let mut parts = vec![];
    for (size, unit) in [
        (86_400, "day"),
        (3600, "hour"),
        (60, "minute"),
        (1, "second"),
    ] {
        let n = left / size;
        left %= size;
        if n > 0 {
            parts.push(format!("{n} {unit}{}", if n == 1 { "" } else { "s" }));
        }
    }
    match parts.as_slice() {
        [one] if one.starts_with("1 ") => one[2..].to_owned(),
        _ => parts.join(" "),
    }
}

/// One reported link as an address label: short link, destination, reason.
fn report_row(slug: &str, url: &str, reason: &str) -> String {
    format!(
        r#"<div class="rule" style="padding:12px 0;border-bottom:1px solid #d5dbd2;font-family:'Courier Prime','Courier New',Courier,monospace;">
<div class="ink" style="font-size:17px;font-weight:700;color:#1a2421;"><span class="soft" style="font-weight:400;color:#5d6b66;">/</span>{}</div>
<div class="soft" style="margin-top:2px;font-size:14px;line-height:20px;color:#5d6b66;word-break:break-all;">{}</div>
<div style="margin-top:6px;"><span class="stamp" style="display:inline-block;border:1.5px solid #2f3fd0;border-radius:5px;padding:0 6px;font-size:12px;line-height:18px;color:#2f3fd0;">{}</span></div>
</div>"#,
        escape(slug),
        escape(url),
        escape(reason_label(reason))
    )
}

/// Matches the labels on the report form.
fn reason_label(reason: &str) -> &str {
    match reason {
        "phishing" => "Phishing or scam",
        "malware" => "Malware",
        "spam" => "Spam",
        "illegal" => "Illegal content",
        "other" => "Something else",
        r => r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spoken_reads_after_in_the_last() {
        let s = |secs| spoken(Duration::from_secs(secs));
        assert_eq!(s(600), "10 minutes");
        assert_eq!(s(3600), "hour");
        assert_eq!(s(60), "minute");
        assert_eq!(s(5400), "1 hour 30 minutes");
        assert_eq!(s(86_400 * 2), "2 days");
        assert_eq!(s(0), "second");
    }
}
