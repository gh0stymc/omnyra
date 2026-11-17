-- Step 2a: live streams observed by Omnyra (Helix only reports viewers "right now", A2).
CREATE TABLE live_sessions (
  id               TEXT PRIMARY KEY,
  account_id       TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  external_id      TEXT,              -- Twitch stream ID (matches a VOD's stream_id)
  title            TEXT,
  category         TEXT,
  started_at       TEXT NOT NULL,
  ended_at         TEXT,              -- NULL while live
  peak_viewers     INTEGER,
  avg_viewers      REAL,
  followers_gained INTEGER,
  vod_content_id   TEXT REFERENCES contents(id),
  UNIQUE (account_id, external_id)
);
CREATE INDEX idx_live_sessions_account ON live_sessions(account_id, started_at);

CREATE TABLE live_samples (
  session_id TEXT NOT NULL REFERENCES live_sessions(id) ON DELETE CASCADE,
  ts         TEXT NOT NULL,
  viewers    INTEGER,
  chat_msgs  INTEGER,                 -- per 10 s bucket (A3, later)
  PRIMARY KEY (session_id, ts)
) WITHOUT ROWID;
