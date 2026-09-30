-- Bind drafts to the saved teacher assessment and preserve them across restarts.
ALTER TABLE submissions ADD COLUMN grade_revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE submissions ADD COLUMN feedback_json TEXT;
ALTER TABLE submissions ADD COLUMN feedback_revision INTEGER;
CREATE INDEX IF NOT EXISTS submissions_assignment_idx ON submissions(assignment_id);
CREATE INDEX IF NOT EXISTS audit_submission_idx ON logs_audit(submission_id, id);
