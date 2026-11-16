-- Omnyra migration 0001: phase 0-1 tables (see docs/ARCHITECTURE.md section 5).
-- IDs are TEXT (UUID v7). Timestamps are ISO-8601 UTC TEXT. Days are 'YYYY-MM-DD' (UTC).

CREATE TABLE files (
  id            TEXT PRIMARY KEY,
  path          TEXT NOT NULL,
  managed       INTEGER NOT NULL,
  original_name TEXT, mime TEXT, size_bytes INTEGER, sha256 TEXT,
  media_info    TEXT,
  created_at    TEXT NOT NULL
);

CREATE TABLE accounts (
  id             TEXT PRIMARY KEY,
  platform       TEXT NOT NULL,
  external_id    TEXT NOT NULL,
  handle         TEXT,
  display_name   TEXT NOT NULL,
  avatar_file_id TEXT REFERENCES files(id),
  granted_scopes TEXT NOT NULL DEFAULT '[]',
  status         TEXT NOT NULL DEFAULT 'active',
  connected_at   TEXT NOT NULL,
  last_sync_at   TEXT,
  settings       TEXT NOT NULL DEFAULT '{}',
  UNIQUE (platform, external_id)
);

CREATE TABLE contents (
  id                TEXT PRIMARY KEY,
  account_id        TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  external_id       TEXT NOT NULL,
  kind              TEXT NOT NULL,
  title             TEXT,
  description       TEXT,
  url               TEXT,
  thumbnail_file_id TEXT REFERENCES files(id),
  published_at      TEXT,
  duration_s        INTEGER,
  aspect            TEXT,
  visibility        TEXT,
  extra             TEXT NOT NULL DEFAULT '{}',
  first_seen_at     TEXT NOT NULL,
  last_fetched_at   TEXT NOT NULL,
  UNIQUE (account_id, external_id)
);
CREATE INDEX idx_contents_account_pub ON contents(account_id, published_at);

CREATE TABLE account_snapshots (
  account_id  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  day         TEXT NOT NULL,
  metric      TEXT NOT NULL,
  value       REAL NOT NULL,
  source      TEXT NOT NULL,
  captured_at TEXT NOT NULL,
  PRIMARY KEY (account_id, metric, day)
) WITHOUT ROWID;

CREATE TABLE content_snapshots (
  content_id  TEXT NOT NULL REFERENCES contents(id) ON DELETE CASCADE,
  day         TEXT NOT NULL,
  metric      TEXT NOT NULL,
  value       REAL NOT NULL,
  source      TEXT NOT NULL,
  captured_at TEXT NOT NULL,
  PRIMARY KEY (content_id, metric, day)
) WITHOUT ROWID;

-- idea_id is added by the migration that creates `ideas` (phase 2b).
CREATE TABLE publications (
  id                   TEXT PRIMARY KEY,
  batch_id             TEXT,
  account_id           TEXT NOT NULL REFERENCES accounts(id),
  source_file_id       TEXT REFERENCES files(id),
  rendition_file_id    TEXT REFERENCES files(id),
  thumbnail_file_id    TEXT REFERENCES files(id),
  title TEXT, description TEXT,
  tags                 TEXT NOT NULL DEFAULT '[]',
  visibility           TEXT,
  platform_options     TEXT NOT NULL DEFAULT '{}',
  status               TEXT NOT NULL,
  schedule_mode        TEXT,
  scheduled_at         TEXT,
  published_content_id TEXT REFERENCES contents(id),
  error_code TEXT, error_detail TEXT,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX idx_publications_sched ON publications(status, scheduled_at);

CREATE TABLE jobs (
  id         TEXT PRIMARY KEY,
  kind       TEXT NOT NULL,
  ref_id     TEXT,
  payload    TEXT NOT NULL DEFAULT '{}',
  status     TEXT NOT NULL,
  attempts   INTEGER NOT NULL DEFAULT 0,
  run_after  TEXT NOT NULL,
  progress   REAL,
  last_error TEXT,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX idx_jobs_pending ON jobs(status, run_after);

CREATE TABLE quota_ledger (
  platform   TEXT NOT NULL,
  quota_day  TEXT NOT NULL,
  units_used INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (platform, quota_day)
) WITHOUT ROWID;

CREATE TABLE api_cache (
  key TEXT PRIMARY KEY, etag TEXT, body BLOB, fetched_at TEXT NOT NULL, expires_at TEXT
);

CREATE TABLE alerts (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL, severity TEXT NOT NULL,
  account_id TEXT REFERENCES accounts(id) ON DELETE CASCADE,
  content_id TEXT REFERENCES contents(id) ON DELETE CASCADE,
  payload TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL, read_at TEXT, notified_at TEXT
);

CREATE TABLE audit_log (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  ts        TEXT NOT NULL,
  action    TEXT NOT NULL,
  account_id TEXT,
  detail    TEXT NOT NULL DEFAULT '{}',
  prev_hash TEXT, hash TEXT
);
CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit_log is append-only'); END;
CREATE TRIGGER audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit_log is append-only'); END;

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
