-- Step 5c: series inside a content (YouTube retention curve) and Twitch chat activity
-- recorded while Omnyra watched the stream (A3), in 10-second buckets.
CREATE TABLE content_series (
  content_id TEXT NOT NULL REFERENCES contents(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL,
  data       TEXT NOT NULL,
  fetched_at TEXT NOT NULL,
  PRIMARY KEY (content_id, kind)
) WITHOUT ROWID;

CREATE TABLE live_chat (
  session_id TEXT NOT NULL REFERENCES live_sessions(id) ON DELETE CASCADE,
  -- start of the 10 s bucket, RFC 3339 UTC
  bucket     TEXT NOT NULL,
  msgs       INTEGER NOT NULL,
  PRIMARY KEY (session_id, bucket)
) WITHOUT ROWID;
