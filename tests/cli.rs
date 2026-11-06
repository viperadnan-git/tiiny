mod common;

use axum::http::StatusCode;
use clap::{Parser, error::ErrorKind};
use common::{client, signed_in, test_app};
use serde_json::{Value, json};
use tiiny::{Cli, CliOutcome, Command, run_cli};

fn parse(a: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("tiiny").chain(a.iter().copied()))
}

fn args(a: &[&str]) -> Cli {
    parse(a).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_password_sets_working_temp_password() {
    let app = test_app(&[]).await;
    let (old, uid) = signed_in(&app, "a@x.com", "user").await;
    common::exec(&app.state.db, "INSERT INTO oauth_accounts (user_id, provider, provider_user_id, created_at) VALUES (?1, 'github', 'gh1', 0)",
            [uid]).await;
    let out = run_cli(
        args(&["reset-password", "A@X.com"]),
        (*app.state.config).clone(),
    )
    .await
    .unwrap();
    let CliOutcome::Done(out) = out else {
        panic!("expected Done")
    };
    let pw = out.trim().rsplit(' ').next().unwrap().to_owned();
    assert_eq!(pw.len(), 16, "{out}");
    assert_eq!(
        old.get("/api/v1/auth/me").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
    let s = client(&app);
    s.post("/api/v1/auth/login")
        .json(&json!({ "email": "a@x.com", "password": pw }))
        .await
        .assert_status_ok();
    let accounts: Value = s.get("/api/v1/auth/accounts").await.json();
    assert!(accounts.as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn promote_admin_sets_role() {
    let app = test_app(&[]).await;
    let (s, _) = signed_in(&app, "a@x.com", "user").await;
    let out = run_cli(
        args(&["promote-admin", "a@x.com"]),
        (*app.state.config).clone(),
    )
    .await
    .unwrap();
    assert!(matches!(out, CliOutcome::Done(_)));
    let me: Value = s.get("/api/v1/auth/me").await.json();
    assert_eq!(me["role"], "admin");
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_email_errors_and_serve_is_default() {
    let app = test_app(&[]).await;
    let cfg = || (*app.state.config).clone();
    for a in [
        args(&["reset-password", "nobody@x.com"]),
        args(&["promote-admin", "nobody@x.com"]),
    ] {
        assert!(run_cli(a, cfg()).await.is_err());
    }
    assert!(matches!(
        run_cli(args(&[]), cfg()).await.unwrap(),
        CliOutcome::Serve(_)
    ));
    assert!(matches!(
        run_cli(args(&["serve"]), cfg()).await.unwrap(),
        CliOutcome::Serve(_)
    ));
}

#[test]
fn help_and_version_exit_zero() {
    for (a, kind) in [
        ("--help", ErrorKind::DisplayHelp),
        ("--version", ErrorKind::DisplayVersion),
    ] {
        let e = parse(&[a]).unwrap_err();
        assert_eq!((e.kind(), e.exit_code()), (kind, 0), "{a}");
    }
    let help = parse(&["--help"]).unwrap_err().to_string();
    for cmd in ["serve", "reset-password", "promote-admin"] {
        assert!(help.contains(cmd), "{help}");
    }
}

#[test]
fn bad_usage_errors() {
    for (a, kind) in [
        (&["frobnicate"][..], ErrorKind::InvalidSubcommand),
        (&["promote-admin"], ErrorKind::MissingRequiredArgument),
        (
            &["reset-password", "a@x.com", "extra"],
            ErrorKind::UnknownArgument,
        ),
        (&["promote-admin", "a@b"], ErrorKind::ValueValidation),
    ] {
        let e = parse(a).unwrap_err();
        assert_eq!((e.kind(), e.exit_code()), (kind, 2), "{a:?}");
    }
    assert!(
        parse(&["reset-password", "a@b"])
            .unwrap_err()
            .to_string()
            .contains("invalid email address")
    );
    let Command::ResetPassword { email } = parse(&["reset-password", " A@X.com "])
        .unwrap()
        .command
        .unwrap()
    else {
        panic!("expected reset-password")
    };
    assert_eq!(email, "a@x.com");
}
