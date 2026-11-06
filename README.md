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

</div>

## Features

- **Short links your way**: pick a slug or get a random one. Links start temporary (302) and editable; make one permanent (301) when it's final.
- **Stats without tracking**: clicks per day, referrer, country, device, browser and OS, kept as daily totals. No IPs, raw user agents or per-visit records.
- **Link preview**: add `+` to any short link (`/abc+`) to see where it goes before opening it.
- **Guest links**: shorten without an account; the links move into the account on sign-up.
- **Sign-in**: password, Google and GitHub; email verification and password reset with SMTP.
- **Abuse handling**: a public report form and an admin queue to block, delete or disable the owner, with undo.
- **Small**: one container, about 9 MB of memory at idle. Data lives in a libSQL file or an embedded replica of a [Turso](https://turso.tech) database.

## Quick start

```sh
docker run -d --name tiiny -p 8080:8080 -v tiiny-data:/data \
  -e TIINY_BASE_URL=https://link.example.com \
  -e TIINY_ADMIN_EMAILS=you@example.com \
  viperadnan/tiiny
```

Open `http://localhost:8080` and sign up with the admin email.

> [!WARNING]
> Whoever signs up first with a listed admin email becomes admin, verified or not. Create your account before opening the server to others.

- **Image**: also `ghcr.io/viperadnan-git/tiiny`, for `linux/amd64` and `linux/arm64`. `latest` follows `main`; releases add `2.1.0`, `2.1` and `2`.
- **Bind mounts**: the container runs as uid `65532`, so `chown 65532:65532` the host directory.
- **Health check**: `GET /healthz` answers `ok`.

## Configuration

Every setting is an environment variable, checked at startup. **[`.env.example`](.env.example) lists them all with defaults.** A `.env` in the working directory is read too; real environment variables win.

- **Client IP**: behind a proxy, set `TIINY_CLIENT_IP_HEADER` to the header it sets (`CF-Connecting-IP`, `X-Real-IP`, `X-Forwarded-For`, …), or rate limits and country stats see the proxy. Only set it if the proxy overwrites that header.
- **OAuth callbacks**: `{TIINY_BASE_URL}/api/v1/auth/oauth/google/callback` and `…/github/callback`.
- **Email**: optional. With SMTP, accounts verify their email before choosing slugs, and admins get a summary of new reports every `TIINY_REPORT_EMAIL_INTERVAL`.

## How links behave

- **Temporary (302)**: destination and slug can change; every click is counted. Every link starts here.
- **Permanent (301)**: can't be edited or made temporary. Browsers cache it for a day, so repeat clicks from one browser that day aren't counted.
- **Deleted** permanent or blocked links retire their slug: it answers `410` until an admin releases it. **Blocked** links answer `451`.
- **Guest links** get a random 7-character slug and a per-IP limit (`TIINY_MAX_GUEST_LINKS_PER_IP_PER_DAY`). Turn them off with `TIINY_GUEST_LINKS_ENABLED=false`.

## Admin and CLI

```text
tiiny [serve]                    run the server (default)
tiiny reset-password <email>     print a temporary password, sign out its sessions, unlink Google/GitHub
tiiny promote-admin <email>      make an existing user an admin
```

In Docker: `docker exec tiiny /tiiny promote-admin you@example.com`. Accounts in `TIINY_ADMIN_EMAILS` are made admin again at every sign-in and restart.

## Limits

- **One instance per database**: the redirect cache, rate limits and click buffer live in memory.
- **Terms of service** are a placeholder: edit `web/src/routes/(marketing)/terms/+page.svelte` before inviting people.

## Development

Needs [Rust](https://rustup.rs) and [bun](https://bun.sh) (npm works too).

```sh
cargo run                               # server on :8080; rebuilds the web app when it changed
cd web && bun run dev                   # hot reload; Vite proxies /api to :8080
cargo test                              # Rust tests
cd web && bun run check && bun run test # web types and tests
cargo run --release --example bench     # the landing page's redirect figures
```

[`AGENTS.md`](AGENTS.md) covers the architecture and conventions.

## Licence

[MIT](LICENSE) © 2026 [Adnan Ahmad](mailto:viperadnan@gmail.com)
