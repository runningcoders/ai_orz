-- Add source_type to artifacts for extensible artifact sources.
-- 1 = attachment, 2 = generated_content, 3 = remote_url (reserved)

ALTER TABLE artifacts ADD COLUMN source_type INTEGER NOT NULL DEFAULT 1;

CREATE INDEX IF NOT EXISTS idx_artifacts_source_type ON artifacts(source_type);
