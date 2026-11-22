-- Step 6a: sponsorship manager. Money is INTEGER cents + currency.
CREATE TABLE sponsorships (
  id             TEXT PRIMARY KEY,
  brand          TEXT NOT NULL,
  contact_name   TEXT,
  contact_email  TEXT,
  -- prospect | negotiating | signed | in_progress | delivered | cancelled
  status         TEXT NOT NULL,
  amount_cents   INTEGER,
  currency       TEXT,
  -- pending | invoiced | paid
  payment_status TEXT NOT NULL DEFAULT 'pending',
  payment_due    TEXT,
  start_date     TEXT,
  end_date       TEXT,
  notes          TEXT,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL
);

CREATE TABLE sponsorship_deliverables (
  id             TEXT PRIMARY KEY,
  sponsorship_id TEXT NOT NULL REFERENCES sponsorships(id) ON DELETE CASCADE,
  description    TEXT NOT NULL,
  platform       TEXT,
  due_at         TEXT,
  done           INTEGER NOT NULL DEFAULT 0,
  position       INTEGER NOT NULL DEFAULT 0,
  publication_id TEXT REFERENCES publications(id) ON DELETE SET NULL
);

CREATE TABLE sponsorship_files (
  sponsorship_id TEXT NOT NULL REFERENCES sponsorships(id) ON DELETE CASCADE,
  file_id        TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  PRIMARY KEY (sponsorship_id, file_id)
);
