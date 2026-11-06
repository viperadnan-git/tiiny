use std::sync::Arc;
#[cfg(any(test, feature = "test-mailer"))]
use std::sync::Mutex;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart},
    transport::smtp::authentication::Credentials,
};
use sha2::{Digest, Sha256};

use crate::{
    config::{Config, SmtpTls},
    db::{Db, now_unix},
    error::AppError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentMail {
    pub to: String,
    pub subject: String,
    pub body: String,
    pub html: String,
}

#[derive(Clone)]
pub enum Mailer {
    Disabled,
    Smtp(Arc<AsyncSmtpTransport<Tokio1Executor>>, Mailbox),
    #[cfg(any(test, feature = "test-mailer"))]
    Capture(Arc<Mutex<Vec<SentMail>>>),
}

impl Mailer {
    pub fn from_config(config: &Config) -> anyhow::Result<Mailer> {
        let Some(c) = &config.smtp else {
            return Ok(Mailer::Disabled);
        };
        if c.tls == SmtpTls::None && c.password.is_some() {
            tracing::warn!("SMTP TLS is disabled; credentials are sent in plaintext");
        }
        let mut b = match c.tls {
            SmtpTls::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&c.host)?,
            SmtpTls::Starttls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&c.host)?,
            SmtpTls::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&c.host),
        }
        .port(c.port);
        if let (Some(u), Some(p)) = (&c.username, &c.password) {
            b = b.credentials(Credentials::new(u.clone(), p.clone()));
        }
        Ok(Mailer::Smtp(Arc::new(b.build()), c.from.parse()?))
    }

    pub fn is_enabled(&self) -> bool {
        !matches!(self, Mailer::Disabled)
    }

    pub async fn send(
        &self,
        to: &str,
        subject: &str,
        body: String,
        html: String,
    ) -> anyhow::Result<()> {
        self.deliver(Some(to), &[], subject, body, html).await
    }

    /// One message to everyone in Bcc, addressed to the sender, so recipients don't see each other.
    pub async fn send_bcc(
        &self,
        bcc: &[String],
        subject: &str,
        body: String,
        html: String,
    ) -> anyhow::Result<()> {
        self.deliver(None, bcc, subject, body, html).await
    }

    async fn deliver(
        &self,
        to: Option<&str>,
        bcc: &[String],
        subject: &str,
        body: String,
        html: String,
    ) -> anyhow::Result<()> {
        match self {
            Mailer::Disabled => anyhow::bail!("mailer disabled"),
            Mailer::Smtp(transport, from) => {
                let msg = message(from, to, bcc, subject, body, html)?;
                let started = std::time::Instant::now();
                transport.send(msg).await?;
                tracing::debug!(
                    subject,
                    recipients = bcc.len().max(1),
                    latency_ms = started.elapsed().as_millis() as u64,
                    "email sent"
                );
                Ok(())
            }
            #[cfg(any(test, feature = "test-mailer"))]
            Mailer::Capture(sent) => {
                sent.lock().unwrap().push(SentMail {
                    to: to.map_or_else(|| bcc.join(", "), Into::into),
                    subject: subject.into(),
                    body,
                    html,
                });
                Ok(())
            }
        }
    }
}

/// With no `to`, the header names the sender and only `bcc` receive it (the envelope), so the
/// sender's mailbox gets no copy and recipients don't see each other.
fn message(
    from: &Mailbox,
    to: Option<&str>,
    bcc: &[String],
    subject: &str,
    body: String,
    html: String,
) -> anyhow::Result<Message> {
    let msg = Message::builder().from(from.clone()).subject(subject);
    let msg = match to {
        Some(to) => msg.to(to.parse()?),
        None => {
            let rcpt: Vec<lettre::Address> = bcc
                .iter()
                .filter_map(|b| {
                    // One bad address must not hold back everyone else's copy.
                    b.parse()
                        .inspect_err(|e| tracing::warn!(error = %e, "skipped unparsable recipient"))
                        .ok()
                })
                .collect();
            msg.to(from.clone())
                .envelope(lettre::address::Envelope::new(
                    Some(from.email.clone()),
                    rcpt,
                )?)
        }
    };
    Ok(msg.multipart(MultiPart::alternative_plain_html(body, html))?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenPurpose {
    VerifyEmail,
    ResetPassword,
}

impl TokenPurpose {
    fn as_str(self) -> &'static str {
        match self {
            Self::VerifyEmail => "verify_email",
            Self::ResetPassword => "reset_password",
        }
    }

    fn ttl_secs(self) -> i64 {
        match self {
            Self::VerifyEmail => 24 * 3600,
            Self::ResetPassword => 3600,
        }
    }
}

pub(crate) fn hash_token(token: &str) -> String {
    use std::fmt::Write;
    Sha256::digest(token.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Entropy of email tokens and guest keys.
pub const SECRET_BYTES: usize = 32;

/// Unpadded base64url of `SECRET_BYTES` from the system RNG; store only its [`hash_token`].
pub(crate) fn random_secret() -> String {
    use rand::TryRng;
    let mut raw = [0u8; SECRET_BYTES];
    rand::rngs::SysRng
        .try_fill_bytes(&mut raw)
        .expect("system RNG");
    URL_SAFE_NO_PAD.encode(raw)
}

/// Replaces any earlier unused token of the same purpose; returns the raw token (only its hash is stored).
pub async fn issue_token(db: &Db, user_id: i64, purpose: TokenPurpose) -> anyhow::Result<String> {
    let token = random_secret();
    let conn = db.conn().await?;
    let tx = conn.transaction().await?;
    tx.execute(
        "DELETE FROM email_tokens WHERE user_id = ?1 AND purpose = ?2 AND used_at IS NULL",
        libsql::params![user_id, purpose.as_str()],
    )
    .await?;
    tx.execute(
        "INSERT INTO email_tokens (user_id, purpose, token_hash, expires_at) VALUES (?1, ?2, ?3, ?4)",
        libsql::params![
            user_id,
            purpose.as_str(),
            hash_token(&token),
            now_unix() + purpose.ttl_secs()
        ],
    )
    .await?;
    tx.commit().await?;
    Ok(token)
}

/// Single-use: the UPDATE claims the token atomically.
/// `only_user` restricts the claim to that user's token, leaving others' tokens unconsumed.
pub async fn consume_token(
    w: &libsql::Connection,
    token: &str,
    purpose: TokenPurpose,
    only_user: Option<i64>,
) -> Result<i64, AppError> {
    let now = now_unix();
    let mut rows = w
        .query(
            "UPDATE email_tokens SET used_at = ?1
             WHERE token_hash = ?2 AND purpose = ?3 AND used_at IS NULL AND expires_at > ?1
               AND (?4 IS NULL OR user_id = ?4)
             RETURNING user_id",
            libsql::params![now, hash_token(token), purpose.as_str(), only_user],
        )
        .await?;
    match rows.next().await? {
        Some(r) => Ok(r.get(0)?),
        None => Err(AppError::BadRequest("invalid or expired token".into())),
    }
}

fn link(config: &Config, path: &str, token: &str) -> String {
    format!("{}/auth/{path}?token={token}", config.origin())
}

/// A rendered email: subject, plain-text body and HTML.
pub struct Mail {
    pub subject: String,
    pub body: String,
    pub html: String,
}

pub fn verification_mail(config: &Config, to: &str, token: &str) -> Mail {
    let url = link(config, "verify", token);
    Mail {
        subject: "Verify your email for tiiny".into(),
        body: format!(
            "Verify your email address for tiiny by opening this link. It works once, within 24 hours.\n\n{url}\n\nDidn't create a tiiny account? Ignore this email.\n"
        ),
        html: Letter {
            to,
            stamp: ("Valid", "24 hr"),
            title: "Verify your email",
            intro: "Verify this address to start making short links on tiiny. The link works once, within 24 hours.",
            rows: String::new(),
            action: "Verify email",
            url: &url,
            note: "Didn't create a tiiny account? Ignore this email; no account is verified without it.",
        }
        .render(config.origin()),
    }
}

pub fn reset_mail(config: &Config, to: &str, token: &str) -> Mail {
    let url = link(config, "reset", token);
    Mail {
        subject: "Reset your tiiny password".into(),
        body: format!(
            "Choose a new password for your tiiny account by opening this link. It works once, within one hour.\n\n{url}\n\nDidn't ask for this? Ignore this email and your password stays the same.\n"
        ),
        html: Letter {
            to,
            stamp: ("Valid", "1 hr"),
            title: "Choose a new password",
            intro: "We got a request to reset the password for your tiiny account. The link works once, within one hour.",
            rows: String::new(),
            action: "Reset password",
            url: &url,
            note: "Didn't ask for this? Ignore this email and your password stays the same.",
        }
        .render(config.origin()),
    }
}

pub async fn send_verification(
    mailer: &Mailer,
    db: &Db,
    config: &Config,
    user_id: i64,
    to: &str,
) -> anyhow::Result<()> {
    let token = issue_token(db, user_id, TokenPurpose::VerifyEmail).await?;
    let m = verification_mail(config, to, &token);
    mailer.send(to, &m.subject, m.body, m.html).await
}

pub async fn send_reset(
    mailer: &Mailer,
    db: &Db,
    config: &Config,
    user_id: i64,
    to: &str,
) -> anyhow::Result<()> {
    let token = issue_token(db, user_id, TokenPurpose::ResetPassword).await?;
    let m = reset_mail(config, to, &token);
    mailer.send(to, &m.subject, m.body, m.html).await
}

/// Paper-themed HTML body. Every field is escaped except `rows`, which is pre-rendered table rows.
pub struct Letter<'a> {
    pub to: &'a str,
    pub stamp: (&'a str, &'a str),
    pub title: &'a str,
    pub intro: &'a str,
    pub rows: String,
    pub action: &'a str,
    pub url: &'a str,
    pub note: &'a str,
}

impl Letter<'_> {
    pub fn render(&self, origin: &str) -> String {
        fill(
            include_str!("letter.html"),
            &[
                ("band", &airmail_band()),
                ("preheader", &escape(self.intro)),
                ("to", &escape(self.to)),
                ("stamp_label", &escape(self.stamp.0)),
                ("stamp_value", &escape(self.stamp.1)),
                ("title", &escape(self.title)),
                ("intro", &escape(self.intro)),
                ("rows", &self.rows),
                ("action", &escape(self.action)),
                ("url", &escape(self.url)),
                ("note", &escape(self.note)),
                ("origin", &escape(origin)),
            ],
        )
    }
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Single pass, so placeholder-like text inside values is never expanded.
fn fill(tpl: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(tpl.len() + 1024);
    let mut rest = tpl;
    while let Some(i) = rest.find("{{") {
        out.push_str(&rest[..i]);
        let j = i + rest[i..].find("}}").expect("unclosed placeholder");
        let key = &rest[i + 2..j];
        let (_, v) = vars
            .iter()
            .find(|(k, _)| *k == key)
            .unwrap_or_else(|| panic!("no value for {key}"));
        out.push_str(v);
        rest = &rest[j + 2..];
    }
    out.push_str(rest);
    out
}

/// Airmail edge: blue, gap, red, gap. Cells, not gradients, so every client draws it.
fn airmail_band() -> String {
    let cells: String = (0..28)
        .map(|i| match i % 4 {
            0 => r#"<td class="blue" style="background:#2f3fd0;"></td>"#,
            2 => r#"<td class="red" style="background:#b3261e;"></td>"#,
            _ => r#"<td class="gap" style="background:#fbfcf8;"></td>"#,
        })
        .collect();
    format!(
        r#"<table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0" style="table-layout:fixed;"><tr style="height:6px;line-height:6px;font-size:0;">{cells}</tr></table>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcc_message_reaches_only_the_recipients() {
        let from: Mailbox = "tiiny <noreply@x.com>".parse().unwrap();
        let bcc = [
            "a1@x.com".to_owned(),
            "not an address".into(),
            "a2@x.com".into(),
        ];
        let m = message(&from, None, &bcc, "s", "b".into(), "<p>b</p>".into()).unwrap();
        let rcpt: Vec<String> = m.envelope().to().iter().map(ToString::to_string).collect();
        assert_eq!(rcpt, ["a1@x.com", "a2@x.com"]);
        let raw = String::from_utf8(m.formatted()).unwrap();
        assert!(raw.contains("To: tiiny <noreply@x.com>"), "{raw}");
        assert!(!raw.contains("a1@x.com"), "recipients leak into headers");
    }

    #[test]
    fn letter_escapes_values_and_fills_every_placeholder() {
        let html = Letter {
            to: "a<b>@x.com",
            stamp: ("Valid", "1 hr"),
            title: "{{url}}",
            intro: "i",
            rows: String::new(),
            action: "Go",
            url: "https://t.test/a?x=1&y=2",
            note: "n",
        }
        .render("https://t.test");
        assert!(html.contains("a&lt;b&gt;@x.com"));
        assert!(html.contains("https://t.test/a?x=1&amp;y=2"));
        assert!(html.contains("<title>{{url}}</title>"));
        assert_eq!(html.matches("{{").count(), 2);
    }
}
