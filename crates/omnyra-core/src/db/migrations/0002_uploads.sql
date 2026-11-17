-- Step 1d: upload queue state on publications (the generic `jobs` scheduler arrives in 2c).
ALTER TABLE publications ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE publications ADD COLUMN retry_at TEXT;
-- 0..1, last known upload progress (also streamed live through an event).
ALTER TABLE publications ADD COLUMN progress REAL;
-- Platform ID of the uploaded content (YouTube video ID).
ALTER TABLE publications ADD COLUMN external_id TEXT;
-- JSON array of warning codes (thumbnail or playlist could not be set...).
ALTER TABLE publications ADD COLUMN warnings TEXT NOT NULL DEFAULT '[]';
CREATE INDEX idx_publications_queue ON publications(status, retry_at);
