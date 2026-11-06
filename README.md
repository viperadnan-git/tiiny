<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="web/static/logo-dark.svg">
  <img src="web/static/logo-light.svg" alt="tiiny logo" width="72" height="72">
</picture>

# tiiny

**Self-hosted link shortener that counts clicks, not people.**
Custom slugs, privacy-friendly stats and an admin panel in one small binary.

[![CI](https://github.com/viperadnan-git/tiiny/actions/workflows/ci.yml/badge.svg)](https://github.com/viperadnan-git/tiiny/actions/workflows/ci.yml)
[![Docker pulls](https://img.shields.io/docker/pulls/viperadnan/tiiny)](https://hub.docker.com/r/viperadnan/tiiny)
[![Image size](https://img.shields.io/docker/image-size/viperadnan/tiiny/latest)](https://hub.docker.com/r/viperadnan/tiiny)
[![License: MIT](https://img.shields.io/github/license/viperadnan-git/tiiny)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built_with-Rust-dea584?logo=rust)](https://www.rust-lang.org)

[Quick start](#quick-start) · [Features](#features) · [Configuration](#configuration) · [Development](#development)

</div>

---

tiiny is a self-hosted link shortener in a single Rust binary. Pick your own slug or take a random one, choose a temporary or permanent redirect, and see clicks by day, referrer, country, device, browser and OS. Only daily totals are kept: no IP addresses, no raw user agents, no record per visit. Accounts, guest links, abuse reports and an admin panel are built in, and data lives in a local libSQL file or a Turso database.

## Features

- **Short links your way**: choose the slug or get a random one; temporary links (302) stay editable, permanent ones (301) never change.
- **Stats without tracking**: clicks per day, referring site, country, device, browser and OS, kept as daily totals only.
- **Guest links**: visitors can shorten a link without an account; it moves into their account when they sign up.
- **Sign-in options**: password, Google and GitHub, with email verification and password reset when SMTP is set.
- **Abuse handling**: a public report form and an admin queue to block, delete or disable the owner; blocks and disables can be undone.
- **Admin panel**: users, every link, guest links, retired slugs and a search palette (<kbd>⌘</kbd> <kbd>K</kbd>).
- **Small and fast**: about 9 MB of memory at idle and redirects in microseconds from memory ([benchmark](#benchmark)).
- **One container**: the web app is embedded in the binary; the database is a file, or an embedded replica of a [Turso](https://turso.tech) primary.

## Quick start

```sh
docker run -d --name tiiny -p 8080:8080 -v tiiny-data:/data \
  -e TIINY_BASE_URL=https://link.example.com \
  -e TIINY_ADMIN_EMAILS=you@example.com \
  viperadnan/tiiny
```

The image is also on GitHub Container Registry as `ghcr.io/viperadnan-git/tiiny`, for `linux/amd64` and `linux/arm64`. Tags: `latest` follows `main`, releases add `1.2.3`, `1.2` and `1`.

Then open `http://localhost:8080` and sign up with the address in `TIINY_ADMIN_EMAILS` to get the admin role.

> [!WARNING]
> Whoever signs up first with a listed admin email becomes admin, verified or not. Create your account before you open the server to others.

Put a TLS-terminating reverse proxy in front, and tell tiiny which header carries the client IP (see [Client IP](#client-ip)). The health check is `GET /healthz`, which answers `ok`.

<details>
<summary>Build the image yourself</summary>

```sh
git clone https://github.com/viperadnan-git/tiiny && cd tiiny
docker build -t viperadnan/tiiny .
# multi-arch; the other architecture builds under QEMU and is slow:
docker buildx build --platform linux/amd64,linux/arm64 -t viperadnan/tiiny .
```

The image is distroless and runs as uid `65532`. `/data` already has that owner; for a bind mount, run `chown 65532:65532` on the host directory.

</details>

## Configuration

Every setting is an environment variable, checked at startup: a bad value stops the server with a message naming the variable.

**[`.env.example`](.env.example) is the reference for all options**, with defaults and notes. Copy it to `.env` and edit it; the file is read from the working directory, and real environment variables win.

```sh
cp .env.example .env
```

### Client IP

Rate limits and country stats need the visitor's IP. Behind a proxy, set `TIINY_CLIENT_IP_HEADER` to the header it sets: `CF-Connecting-IP`, `X-Real-IP`, `True-Client-IP`, `Fly-Client-IP`, `CloudFront-Viewer-Address`, `X-Envoy-External-Address`, or `X-Forwarded-For` (its rightmost value, so exactly one proxy). Unset, tiiny uses the connection's address and logs a warning when that looks like a proxy.

> [!NOTE]
> Only set it behind a proxy that overwrites the header, or clients can fake their IP. Cloudflare's `CF-IPCountry` is read only with `CF-Connecting-IP`.

### OAuth

Register these callback URLs with the provider:

- `{TIINY_BASE_URL}/api/v1/auth/oauth/google/callback`
- `{TIINY_BASE_URL}/api/v1/auth/oauth/github/callback`

### Email

SMTP is optional. Without it, no verification or reset emails are sent: reset passwords with the CLI. With it, accounts verify their email before choosing slugs, and admins get one summary email of new abuse reports per `TIINY_REPORT_EMAIL_INTERVAL`.

## Running it

### Admin and CLI

The binary doubles as a CLI, for example `docker exec tiiny /tiiny promote-admin you@example.com`:

```text
tiiny [serve]                    run the server (default)
tiiny reset-password <email>     print a temporary password, sign out its sessions, unlink Google/GitHub
tiiny promote-admin <email>      make an existing user an admin
tiiny --help | --version
```

Accounts in `TIINY_ADMIN_EMAILS` are made admin again at every sign-in and restart; remove an address from the list to demote it for good.

### How links behave

- **Temporary (302)** links can change destination and slug, and every click is counted. Every link starts temporary; make it permanent from its menu.
- **Permanent (301)** links can't be edited or made temporary. Browsers cache them for a day, so repeat clicks from one browser that day aren't counted.
- **Deleting** a permanent or blocked link retires its slug: it answers `410 Gone` and can't be reused until an admin releases it.
- **Blocked** links answer `451` with a "disabled for abuse" page.
- Destinations need `http://` or `https://`.

<details>
<summary>Guest links</summary>

- On by default. A visitor without an account gets a random 7-character slug, always temporary, listed under "recent links" in that browser.
- Limited per IP: a burst of 10, then one every 6 s, plus `TIINY_MAX_GUEST_LINKS_PER_IP_PER_DAY` (set `0` to turn guest links off).
- Identity is a random `HttpOnly` cookie (`__Host-tiiny_guest` on https); only its hash is stored, and redirects never read it.
- Signing up or in from the same browser moves its guest links into the account.
- With SMTP set, unverified accounts create links like guests until they verify.

</details>

<details>
<summary>Abuse reports</summary>

- Anyone can report a link at `/report`; the footer and the 404 and 410 pages link to it.
- Admins work the queue at `/admin/reports`. Resolving a report acts on every open report for that link: block, delete, disable the owner, or dismiss.
- Actioned and dismissed reports can be reopened, which undoes what the action changed. Deleted links can't come back.

</details>

### Limits

- **One instance per database.** The redirect cache, rate limits and click buffer live in memory, so don't run several copies against one database.
- **Turso**: reads come from the local replica, writes go to the primary, and the replica catches up after each request that wrote. CLI commands talk to the primary directly.
- **Terms of service** are a placeholder: edit `web/src/routes/(marketing)/terms/+page.svelte` before inviting people.

## Development

Needs [Rust](https://rustup.rs) and [bun](https://bun.sh) (npm works as a fallback).

```sh
cargo run                   # server on :8080; build.rs rebuilds the web app when it changed
cd web && bun run dev       # hot reload; Vite proxies /api to :8080
```

| Task | Command |
|---|---|
| Rust tests | `cargo test` |
| Web type check and tests | `cd web && bun run check && bun run test` |
| Benchmark | `cargo run --release --example bench` |

The version lives only in `Cargo.toml`; the server and the web app both read it from there. [`AGENTS.md`](AGENTS.md) describes the architecture and conventions.

### Benchmark

The landing page's figures come from [`examples/bench.rs`](examples/bench.rs). It starts the release build on a throwaway database with 100,000 links and reports the server's own time per redirect, from memory and from the database (median and p99), plus the time until `/healthz` answers.

## Licence

[MIT](LICENSE) © 2026 [Adnan Ahmad](mailto:viperadnan@gmail.com)
