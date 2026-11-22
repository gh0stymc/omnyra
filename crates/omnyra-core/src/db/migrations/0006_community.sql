-- Step 4a/4b: unified comment inbox, rule-based classification and repeated questions.
CREATE TABLE question_clusters (
  id         TEXT PRIMARY KEY,
  label      TEXT NOT NULL,
  size       INTEGER NOT NULL,
  idea_id    TEXT REFERENCES ideas(id) ON DELETE SET NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE comments (
  id                 TEXT PRIMARY KEY,
  account_id         TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  content_id         TEXT REFERENCES contents(id) ON DELETE CASCADE,
  external_id        TEXT NOT NULL,
  parent_id          TEXT REFERENCES comments(id) ON DELETE CASCADE,
  author_name        TEXT,
  author_external_id TEXT,
  is_own             INTEGER NOT NULL DEFAULT 0,
  text               TEXT NOT NULL,
  published_at       TEXT,
  like_count         INTEGER,
  -- pending | replied | archived
  status             TEXT NOT NULL DEFAULT 'pending',
  -- question | spam | negative | positive | neutral
  category           TEXT,
  -- rules | manual
  category_source    TEXT,
  phishing_score     REAL,
  phishing_reasons   TEXT,
  cluster_id         TEXT REFERENCES question_clusters(id) ON DELETE SET NULL,
  fetched_at         TEXT NOT NULL,
  UNIQUE (account_id, external_id)
);
CREATE INDEX idx_comments_inbox ON comments(status, published_at);
CREATE INDEX idx_comments_parent ON comments(parent_id);
CREATE INDEX idx_comments_cluster ON comments(cluster_id);
