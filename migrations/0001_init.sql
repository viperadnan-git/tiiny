CREATE TABLE users (
  id                INTEGER PRIMARY KEY,
  email             TEXT NOT NULL CHECK (email = lower(email)),
  email_verified_at INTEGER,
  password_hash     TEXT,
  name              TEXT,
  role              TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin')),
  disabled_at       INTEGER,
  created_at        INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX uq_users_email ON users(email);

CREATE TABLE oauth_accounts (
  id               INTEGER PRIMARY KEY,
  user_id          INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  provider         TEXT NOT NULL CHECK (provider IN ('google', 'github')),
  provider_user_id TEXT NOT NULL,
  created_at       INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX uq_oauth_accounts_provider_provider_user_id ON oauth_accounts(provider, provider_user_id);
CREATE UNIQUE INDEX uq_oauth_accounts_user_id_provider ON oauth_accounts(user_id, provider);

CREATE TABLE sessions (
  id         TEXT PRIMARY KEY,
  payload    BLOB NOT NULL,
  user_id    INTEGER,
  expires_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_sessions_expires_at ON sessions(expires_at);
CREATE INDEX idx_sessions_user_id ON sessions(user_id);

CREATE TABLE email_tokens (
  id         INTEGER PRIMARY KEY,
  user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  purpose    TEXT NOT NULL CHECK (purpose IN ('verify_email', 'reset_password')),
  token_hash TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  used_at    INTEGER
) STRICT;
CREATE UNIQUE INDEX uq_email_tokens_token_hash ON email_tokens(token_hash);

CREATE TABLE links (
  id             INTEGER PRIMARY KEY,
  user_id        INTEGER REFERENCES users(id) ON DELETE CASCADE,
  guest_key_hash TEXT,
  slug           TEXT NOT NULL,
  url            TEXT NOT NULL,
  click_count    INTEGER NOT NULL DEFAULT 0 CHECK (click_count >= 0),
  permanent_at   INTEGER,
  blocked_at     INTEGER,
  created_at     INTEGER NOT NULL,
  updated_at     INTEGER NOT NULL,
  CHECK ((user_id IS NULL) != (guest_key_hash IS NULL)),
  CHECK (user_id IS NOT NULL OR permanent_at IS NULL)
) STRICT;
CREATE INDEX idx_links_guest_key_hash ON links(guest_key_hash) WHERE guest_key_hash IS NOT NULL;
CREATE UNIQUE INDEX uq_links_slug ON links(slug);
-- Covers the redirect lookup, so a cache miss reads only the index.
CREATE INDEX idx_links_slug_redirect ON links(slug, url, permanent_at, blocked_at, user_id);
CREATE INDEX idx_links_user_id ON links(user_id);

-- Links created per day, for the daily creation limits; purged hourly past yesterday.
-- `owner` is `u:<user id>` or `ip:<ip key>` (no foreign key: deleting a user deletes its rows).
CREATE TABLE link_quota (
  owner     TEXT NOT NULL,
  epoch_day INTEGER NOT NULL,
  created   INTEGER NOT NULL CHECK (created >= 0),
  PRIMARY KEY (owner, epoch_day)
) STRICT;

CREATE TABLE abuse_reports (
  id            INTEGER PRIMARY KEY,
  link_id       INTEGER REFERENCES links(id) ON DELETE SET NULL,
  slug          TEXT NOT NULL,
  url           TEXT NOT NULL,
  reason        TEXT NOT NULL CHECK (reason IN ('phishing', 'malware', 'spam', 'illegal', 'other')),
  details       TEXT,
  contact_email TEXT,
  status        TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'actioned', 'dismissed')),
  action        TEXT CHECK (action IN ('blocked', 'deleted', 'owner_disabled')),
  resolved_by   INTEGER REFERENCES users(id) ON DELETE SET NULL,
  resolved_at   INTEGER,
  blocked_link   INTEGER NOT NULL DEFAULT 0 CHECK (blocked_link IN (0, 1)),
  disabled_owner INTEGER NOT NULL DEFAULT 0 CHECK (disabled_owner IN (0, 1)),
  created_at    INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_abuse_reports_status ON abuse_reports(status, id);
CREATE INDEX idx_abuse_reports_link_id ON abuse_reports(link_id);

CREATE TABLE link_stats (
  link_id INTEGER NOT NULL REFERENCES links(id) ON DELETE CASCADE,
  epoch_day       INTEGER NOT NULL,
  dimension       INTEGER NOT NULL CHECK (dimension BETWEEN 0 AND 5),
  dimension_value TEXT NOT NULL,
  click_count     INTEGER NOT NULL CHECK (click_count >= 0),
  PRIMARY KEY (link_id, epoch_day, dimension, dimension_value)
) STRICT;

CREATE TABLE retired_slugs (
  slug       TEXT PRIMARY KEY,
  retired_at INTEGER NOT NULL
) STRICT;
