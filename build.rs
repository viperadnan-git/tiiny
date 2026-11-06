//! Rebuilds `web/build` (embedded by `frontend::Assets`) when the web sources change.
//! Uses bun, else npm. Never fails the Rust build: problems are cargo warnings and the existing
//! `web/build` is served. Skipped without `web/src` (the Docker Rust stage copies a prebuilt one).

use std::{path::Path, process::Command};

fn main() {
    for p in [
        "web/src",
        "web/static",
        "web/package.json",
        "web/bun.lock",
        "web/vite.config.ts",
        "Cargo.toml",
        // Embedded by `db::MigrationFiles`: a new file must rebuild the binary.
        "migrations",
    ] {
        println!("cargo:rerun-if-changed={p}");
    }
    if !Path::new("web/src").exists() {
        return;
    }
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let Some(pm) = ["bun", npm].into_iter().find(|pm| run(pm, &["--version"])) else {
        return warn("neither bun nor npm found: serving the existing web/build");
    };
    if !Path::new("web/node_modules").exists() {
        let install: &[&str] = if pm == "bun" {
            &["install", "--frozen-lockfile"]
        } else {
            &["install", "--no-package-lock"]
        };
        if !run(pm, install) {
            return warn(&format!(
                "`{pm} install` failed in web/: serving the existing web/build"
            ));
        }
    }
    if !run(pm, &["run", "build"]) {
        warn(&format!(
            "web build failed: run `cd web && {pm} run build` for the errors; serving the existing web/build"
        ));
    }
}

fn run(pm: &str, args: &[&str]) -> bool {
    Command::new(pm)
        .args(args)
        .current_dir("web")
        .status()
        .is_ok_and(|s| s.success())
}

fn warn(msg: &str) {
    println!("cargo:warning={msg}");
}
