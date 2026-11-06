# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

`CLAUDE.md` is a symlink to this file (`AGENTS.md`); edit this one.

tiiny is a self-hosted link shortener: one Rust binary (axum + embedded libSQL) that serves redirects, the JSON API and a SvelteKit web app embedded from `web/build`.

## Commands

```sh
cargo run                                # server on :8080; build.rs rebuilds web/ (bun, else npm) when its sources change; failures are warnings
cd web && bun run dev                    # hot reload; Vite proxies /api to :8080 (API_PROXY=http://localhost:8091 to change)

cargo test                               # all Rust tests
cargo run --release --example bench      # landing page redirect/startup figures (examples/bench.rs)
cargo test --test links_api              # one test file (tests/*.rs)
cargo test --test auth_password admin_email_promoted_at_signup   # one test
cd web && bun run check                  # svelte-check (types)
cd web && bun run test                   # scripts/check-safe-next.ts + scripts/check-schemas.ts
```

A `.env` in the working directory is loaded at startup (real env wins).

The version lives only in `Cargo.toml`; `web/vite.config.ts` reads it from there (the Dockerfile copies `Cargo.toml` into the web stage for this).

## Architecture

- **Config** (`src/config.rs`): every setting is a `TIINY_*` env var (except `HOST` and `PORT`), parsed by figment and validated by garde at startup; errors name the variable and exit. Durations go through `config::parse_duration`: bare seconds or humantime units (`90s`, `1h30m`, `7d`); name them without a unit (`_INTERVAL`, `_TTL`). A renamed variable goes in `RENAMED`, so its old name fails startup instead of being ignored. All variables with defaults and notes are in `.env.example`; keep it in sync when adding one.
- **Routing** (`src/lib.rs` `build_router`): `/api/v1/*` (auth, links, admin, reports: public `POST /reports` and the admin queue under `/admin/reports`), `/healthz`, and a fallback (`src/frontend.rs`) that serves embedded assets, prerendered pages, the SPA shell (`200.html`) for app paths (`/login`, `/dashboard`, `/admin`, …), and otherwise treats the path as a slug (`src/redirect.rs`).
- **Redirects**: `redirect.rs` → `cache::SlugCache` (in-memory) → DB on miss. Each click is recorded by `stats::collector` (buffered, flushed in batches) as daily tallies per dimension; no per-visit rows, IPs or raw user agents. `/{slug}+` previews the destination (no redirect, no click). Permanent links answer 301, temporary 302; deleted permanent or blocked slugs are retired and answer 410 (admins can release them); admin-blocked links answer 451.
- **Guests** (`src/guest.rs`): guest cookie (`__Host-tiiny_guest` on https), separate guest and report rate limiters, and claim of guest links into an account on sign-in.
- **Reports** (`src/reports/`): public `POST /reports`, admin queue and resolve actions, reopen/undo of resolved reports, and the notifier (one Bcc summary email to all admins every `TIINY_REPORT_EMAIL_INTERVAL`).
- **Database** (`src/db.rs`): `db.conn()` returns a `Conn` guard; bind it (`let conn = db.conn().await?`) so rows drop first. In replica mode it is pooled, writes go to the primary, and the local file catches up once per unit of work (`read_your_writes` is off): before the next `conn()` and before the response (`sync_writes` middleware). That only works if writes go through the guard's `query`/`execute`/`transaction*` or `writer()`; pass `reader()` to helpers that only read.
- **Single instance only**: cache, rate limiter, stats buffer and replica catch-up state are in-process.
- **Logging** (`src/logging.rs`): `tracing` to stdout (text) and `<data_dir>/logs` (JSON lines, daily files, 7 kept, `latest.log` symlink) via `tracing-appender`. Each request gets an `x-request-id` and an INFO `request` span (id, method, path), so handler events carry them. Levels: account/admin/report events `info`, link CRUD, mail and per-request summaries `debug`, slow requests (≥1s) `warn`, `AppError::Internal` `error`. Fields are ids, never IPs or email addresses. Filter: `TIINY_LOG`, default `logging::DEFAULT_FILTER`.
- **Auth** (`src/auth/`): axum-login sessions in libSQL; password (argon2), Google/GitHub OAuth, optional SMTP for verification/reset. `promote_listed` in `backend.rs` makes `TIINY_ADMIN_EMAILS` accounts admin at startup, signup, login, verify and OAuth.
- **Mutations require JSON**: `require_json_for_mutations` rejects non-JSON POST/PATCH/PUT/DELETE (415), including body-less DELETEs. Request structs use `deny_unknown_fields`, so the client must strip client-only fields (e.g. `confirm`).
- **Pagination**: keyset by id, newest first. `PageQuery` + `paginate()` in `src/links/mod.rs` fetch `limit + 1` rows; `next_cursor` is the last id. Reuse these for any new list endpoint.
- **CLI** (`src/lib.rs` `Command`): `serve` (default), `reset-password <email>`, `promote-admin <email>`.

### Web (`web/`, SvelteKit + Svelte 5 runes, Tailwind v4, shadcn-svelte)

- Layout: `lib/` holds shared code only; a component or helper used by one route (or one route group) lives next to it as a non-`+` file (e.g. `dashboard/links/[slug]/ClicksChart.svelte`, `(marketing)/SiteHeader.svelte`). Move it to `lib/` once a second route needs it.
- Route groups: `(marketing)` is prerendered, `(status)` is prerendered without JS (404/410, and 451 `blocked` for links disabled for abuse), `(app)` is a client-only SPA (`ssr = false`) whose layout loads `me` and `providers`.
- Prerendered pages carry `%%TIINY_BASE_URL%%`; the server substitutes each instance's origin at startup (empty when unset: relative URLs).
- Origin, single source each side: server `Config::origin()`/`https()` (+ `ShortOrigin` extractor, which falls back to the request Host for `short_url` only); client `lib/origin.ts` (`baseUrl`, `shortHost`, `shortUrl`). Never build a short URL or read `location.host` for one elsewhere.
- `lib/schemas.ts` mirrors the server's validation rules and messages (`src/validate.rs`, `src/links/slug.rs`); keep them in sync, and `scripts/check-schemas.ts` guards them. The server stays authoritative.
- `lib/api-client.ts` (`api()`) is the only fetch wrapper; `lib/forms.ts` (`spaForm`) wires zod schemas to superforms and maps server errors to fields; `lib/paged.svelte.ts` (`paged()`) drives every paginated list, rendered with `ListSearch` + `ListStatus`.
- Theme ("Paper"): tokens in `src/routes/layout.css` (`--paper`, `--sheet`, `--ink`, `--stamp`, …) with light and dark values; fonts Playfair Display (headings), Public Sans (UI), Courier Prime (slugs, URLs, addresses). The visual language is postal: tickets, address labels, rubber stamps (`AddressLabel`, `Stamp`, `Envelope`).

## Rules

- JavaScript: `bun` only. Python: `uv` only.
- Never commit unless explicitly told to "commit". Commit messages, PR titles and bodies carry no trailers or attribution (no Co-Authored-By, no tool/model names); add a body only when needed.
- Comments, docstrings, help text: short, reference style, no narration. Default to none; comment only a non-obvious why.
- **shadcn first**: before building any UI in `web/`, check shadcn-svelte. If it has the component, install it (`bunx shadcn-svelte@latest add <name>` from `web/`; it may try to overwrite existing themed components, so back up and restore them), adapt it to the Paper theme, and use it. No hand-rolled markup for something shadcn provides; compose app components from shadcn primitives.
- **Skeletons match the UI exactly**: every loading state renders the same component structure, sizes, spacing, radii and borders as the loaded UI, so nothing shifts when data arrives. Never write a separate skeleton layout. The real component takes `null` data (e.g. `LinkLabel link={null}`, `BarList items={null}`, `ClicksChart series={null}`) and renders its own markup with stand-in values from `lib/placeholders.ts`, wrapping each data element in `<Skeleton loading={…}>…</Skeleton>`. While loading, `Skeleton` renders the real content invisibly inside the shimmer block (exact size and line metrics); when `loading` is false it renders the children unwrapped. Use `class="inline-block"` for text. Keep layout classes (margins) on a parent, not on the wrapped element, or they apply twice. Lists whose row count varies reserve a fixed minimum of rows with invisible fillers (see `BarList`). The shimmer is themed (`--shimmer` in `layout.css`) and is hidden under reduced motion.
- Mobile first: every page must work from 360px wide with no horizontal scroll.
- User-facing text says what happened or what to do next, from the user's side: plain words, sentence case, active voice, no filler or vague errors, one name per action throughout its flow, and user values (slugs, emails) shown exactly as entered.
