-- Step 6b: public channels the user follows, their daily snapshots and latest videos,
-- and keyword trends.
CREATE TABLE tracked_accounts (
  id           TEXT PRIMARY KEY,
  platform     TEXT NOT NULL,
  external_id  TEXT NOT NULL,
  handle       TEXT,
  display_name TEXT,
  added_at     TEXT NOT NULL,
  refreshed_at TEXT,
  UNIQUE (platform, external_id)
);

CREATE TABLE tracked_snapshots (
  tracked_id TEXT NOT NULL REFERENCES tracked_accounts(id) ON DELETE CASCADE,
  day        TEXT NOT NULL,
  metric     TEXT NOT NULL,
  value      REAL NOT NULL,
  PRIMARY KEY (tracked_id, metric, day)
) WITHOUT ROWID;

CREATE TABLE tracked_contents (
  id           TEXT PRIMARY KEY,
  tracked_id   TEXT NOT NULL REFERENCES tracked_accounts(id) ON DELETE CASCADE,
  external_id  TEXT NOT NULL,
  title        TEXT,
  published_at TEXT,
  views        REAL,
  likes        REAL,
  comments     REAL,
  fetched_at   TEXT NOT NULL,
  UNIQUE (tracked_id, external_id)
);

CREATE TABLE trend_keywords (
  id         TEXT PRIMARY KEY,
  keyword    TEXT NOT NULL,
  platform   TEXT NOT NULL DEFAULT 'youtube',
  created_at TEXT NOT NULL,
  UNIQUE (platform, keyword)
);

CREATE TABLE trend_results (
  keyword_id TEXT NOT NULL REFERENCES trend_keywords(id) ON DELETE CASCADE,
  day        TEXT NOT NULL,
  data       TEXT NOT NULL,
  PRIMARY KEY (keyword_id, day)
) WITHOUT ROWID;
