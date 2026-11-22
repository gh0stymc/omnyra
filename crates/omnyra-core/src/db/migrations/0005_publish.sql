-- Step 3c: multi-network publishing. A publication waiting for its FFmpeg rendition has
-- status 'rendering' and the adaptation to apply here (JSON RenditionSpec); once
-- rendered, `rendition_file_id` points to the managed output and status becomes 'queued'.
ALTER TABLE publications ADD COLUMN render_spec TEXT;
CREATE INDEX idx_publications_batch ON publications(batch_id);
