-- Classes with rosters, per-submission source metadata, teacher-approved feedback,
-- rubric performance levels and identity purging for retention.

CREATE TABLE IF NOT EXISTS classes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Roster names are personal data; they live only in the encrypted workspace.
CREATE TABLE IF NOT EXISTS students (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    class_id INTEGER NOT NULL REFERENCES classes(id) ON DELETE CASCADE,
    full_name TEXT NOT NULL,
    UNIQUE(class_id, full_name)
);
CREATE INDEX IF NOT EXISTS students_class_idx ON students(class_id);

ALTER TABLE assignments ADD COLUMN title TEXT;
ALTER TABLE assignments ADD COLUMN class_id INTEGER REFERENCES classes(id) ON DELETE SET NULL;

ALTER TABLE alias_student_map ADD COLUMN student_id INTEGER REFERENCES students(id) ON DELETE SET NULL;

ALTER TABLE criteria_rubric ADD COLUMN levels_json TEXT;

ALTER TABLE submissions ADD COLUMN source_name TEXT;
ALTER TABLE submissions ADD COLUMN source_sha256 TEXT;
ALTER TABLE submissions ADD COLUMN text_initial TEXT;
ALTER TABLE submissions ADD COLUMN feedback_final TEXT;
ALTER TABLE submissions ADD COLUMN feedback_ai_assisted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE submissions ADD COLUMN identities_purged_at TEXT;

-- Per-submission metadata previously lived in generic configuration keys, which
-- deleting a submission could not cascade to.
UPDATE submissions SET source_sha256 = (
    SELECT value FROM configuration WHERE key = 'document.source.' || submissions.id
);
UPDATE submissions SET text_initial = (
    SELECT value FROM configuration WHERE key = 'document.original_text.' || submissions.id
);
DELETE FROM configuration
WHERE key LIKE 'document.source.%' OR key LIKE 'document.original_text.%';

-- Confirmed grades already exported the AI draft as their feedback; keep that text.
UPDATE submissions
SET feedback_final = json_extract(feedback_json, '$.comment_feedback'), feedback_ai_assisted = 1
WHERE status_pipeline = 'grade_confirmed'
  AND feedback_json IS NOT NULL
  AND feedback_revision = grade_revision;
