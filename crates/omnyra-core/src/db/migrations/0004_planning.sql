-- Step 2b: idea board, reminders and the link from publications to ideas.
CREATE TABLE ideas (
  id               TEXT PRIMARY KEY,
  title            TEXT NOT NULL,
  notes            TEXT,                          -- Markdown
  stage            TEXT NOT NULL DEFAULT 'idea',  -- idea | script | recorded | edited | published
  position         REAL NOT NULL,                 -- order inside the column (fractional)
  target_platforms TEXT NOT NULL DEFAULT '[]',
  due_at           TEXT,
  -- Repeated-question cluster it came from (4b). No FK yet: that table does not exist.
  source_cluster_id TEXT,
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL
);
CREATE INDEX idx_ideas_stage ON ideas(stage, position);

CREATE TABLE idea_files (
  idea_id TEXT NOT NULL REFERENCES ideas(id) ON DELETE CASCADE,
  file_id TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  PRIMARY KEY (idea_id, file_id)
);

-- Local reminders. The calendar is the union of contents.published_at,
-- publications.scheduled_at, reminders.at and ideas.due_at.
CREATE TABLE reminders (
  id             TEXT PRIMARY KEY,
  title          TEXT NOT NULL,
  at             TEXT NOT NULL,
  platform       TEXT,
  idea_id        TEXT REFERENCES ideas(id) ON DELETE CASCADE,
  publication_id TEXT REFERENCES publications(id) ON DELETE CASCADE,
  notified_at    TEXT
);
CREATE INDEX idx_reminders_at ON reminders(at);

ALTER TABLE publications ADD COLUMN idea_id TEXT REFERENCES ideas(id) ON DELETE SET NULL;
