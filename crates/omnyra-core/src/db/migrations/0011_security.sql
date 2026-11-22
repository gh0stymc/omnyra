-- Step 6c: account security checklist (manual answers) and impersonation suspects.
CREATE TABLE security_checklist (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  item       TEXT NOT NULL,
  -- unknown | ok | warning | not_applicable
  state      TEXT NOT NULL,
  source     TEXT NOT NULL,
  checked_at TEXT,
  PRIMARY KEY (account_id, item)
) WITHOUT ROWID;

CREATE TABLE impersonation_hits (
  id                  TEXT PRIMARY KEY,
  account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  platform            TEXT NOT NULL,
  suspect_external_id TEXT NOT NULL,
  suspect_name        TEXT,
  name_score          REAL,
  -- comments | search
  found_in            TEXT NOT NULL,
  -- new | dismissed | reported
  status              TEXT NOT NULL DEFAULT 'new',
  found_at            TEXT NOT NULL,
  UNIQUE (account_id, platform, suspect_external_id)
);
