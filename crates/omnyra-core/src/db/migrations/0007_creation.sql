-- Phase 5: creator's asset library, thumbnail templates (scene JSON of the editor) and
-- text templates for the text assistant.
CREATE TABLE assets (
  id         TEXT PRIMARY KEY,
  file_id    TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  -- avatar | skin | sticker | background | image
  kind       TEXT NOT NULL,
  name       TEXT NOT NULL,
  variant    TEXT,
  tags       TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL
);

CREATE TABLE thumbnail_templates (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  width      INTEGER NOT NULL,
  height     INTEGER NOT NULL,
  scene      TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE text_templates (
  id       TEXT PRIMARY KEY,
  -- NULL = any network
  platform TEXT,
  -- title | description | hashtags
  field    TEXT NOT NULL,
  name     TEXT NOT NULL,
  body     TEXT NOT NULL,
  created_at TEXT NOT NULL
);
