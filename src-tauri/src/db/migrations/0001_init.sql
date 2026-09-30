-- Initial schema (Milestone 2). See docs/ARCHITECTURE.md for the full design of the pipeline and docs/DPIA-FRIA.md for the reasons of each minimization measure (e.g. why the original documents do not live here).

CREATE TABLE IF NOT EXISTS rubrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    subject TEXT NOT NULL,
    grade_level TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    content_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS criteria_rubric (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    rubric_id INTEGER NOT NULL REFERENCES rubrics(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    description TEXT NOT NULL,
    score_max REAL NOT NULL,
    sort_order INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS assignments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    rubric_id INTEGER NOT NULL REFERENCES rubrics(id) ON DELETE CASCADE,
    text TEXT NOT NULL,
    materials_ref TEXT
);

-- Original documents are never stored here (see the privacy boundary in
-- ARCHITECTURE.md). They are processed outside the database.
CREATE TABLE IF NOT EXISTS submissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    assignment_id INTEGER NOT NULL REFERENCES assignments(id) ON DELETE CASCADE,
    alias TEXT UNIQUE,
    text_ocr TEXT,
    method_ocr TEXT,
    text_redacted TEXT,
    status_pipeline TEXT NOT NULL DEFAULT 'imported',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- The most sensitive table: random alias (not sequential) -> real identity. It should never be exported or leave the device.
CREATE TABLE IF NOT EXISTS alias_student_map (
    alias TEXT PRIMARY KEY,
    student_name TEXT NOT NULL,
    student_class_id TEXT,
    submission_id INTEGER NOT NULL REFERENCES submissions(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    submission_id INTEGER NOT NULL REFERENCES submissions(id) ON DELETE CASCADE,
    criterion_id INTEGER NOT NULL REFERENCES criteria_rubric(id),
    ai_evidence_json TEXT,
    -- Reserved for a future AI-assisted grading mode outside Phase A.
    score_suggested REAL,
    score_final REAL,
    ai_comment TEXT,
    comment_teacher TEXT,
    confirmed_by_teacher INTEGER NOT NULL DEFAULT 0,
    confirmed_at TEXT,
    UNIQUE(submission_id, criterion_id)
);

-- Audit trail required by Article 12 of the AI Act.
CREATE TABLE IF NOT EXISTS logs_audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    submission_id INTEGER REFERENCES submissions(id) ON DELETE SET NULL,
    event TEXT NOT NULL,
    actor TEXT NOT NULL,
    model_version TEXT,
    payload_json TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS configuration (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
