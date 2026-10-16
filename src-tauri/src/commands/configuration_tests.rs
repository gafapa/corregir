use super::*;
use crate::models::NewCriterion;
fn rubric(maximum: f64) -> NewRubric {
    NewRubric {
        title: "Custom rubric".into(),
        subject: "Science".into(),
        grade_level: "Year 8".into(),
        criteria: vec![NewCriterion {
            code: "C1".into(),
            description: "Explains the result".into(),
            score_max: maximum,
            levels: vec![],
        }],
    }
}
#[test]
fn rubric_versions_preserve_existing_assignment_criteria_and_scores() {
    let directory = tempfile::tempdir().unwrap();
    let db = crate::db::schema::open_with_key(directory.path(), [6; 32]).unwrap();
    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    let original = insert_rubric(conn, &rubric(10.0), 1).unwrap();
    conn.execute(
        "INSERT INTO assignments (rubric_id,text) VALUES (?1,'Synthetic task')",
        [original],
    )
    .unwrap();
    let assignment = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO submissions (assignment_id,status_pipeline) VALUES (?1,'grade_confirmed')",
        [assignment],
    )
    .unwrap();
    let submission = conn.last_insert_rowid();
    let criterion: i64 = conn
        .query_row(
            "SELECT id FROM criteria_rubric WHERE rubric_id=?1",
            [original],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute("INSERT INTO results(submission_id,criterion_id,score_final,confirmed_by_teacher) VALUES (?1,?2,8,1)",rusqlite::params![submission,criterion]).unwrap();
    let revised = create_rubric_version(conn, original, 1, &rubric(4.0)).unwrap();
    assert_ne!(revised, original);
    let old:(f64,f64)=conn.query_row("SELECT c.score_max,r.score_final FROM criteria_rubric c JOIN results r ON r.criterion_id=c.id WHERE r.submission_id=?1",[submission],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_eq!(old, (10.0, 8.0));
    let new:(i64,f64)=conn.query_row("SELECT version,score_max FROM rubrics r JOIN criteria_rubric c ON c.rubric_id=r.id WHERE r.id=?1",[revised],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_eq!(new, (2, 4.0));
    assert!(create_rubric_version(conn, original, 2, &rubric(5.0)).is_err());
    assert_eq!(
        conn.query_row("SELECT count(*) FROM rubrics", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn rejects_duplicate_codes_and_nonfinite_or_oversized_rubrics() {
    for value in [0.0, -1.0, f64::INFINITY, f64::NAN, 10001.0] {
        assert!(validate_rubric(&rubric(value)).is_err());
    }
    let mut duplicate = rubric(10.0);
    duplicate.criteria.push(duplicate.criteria[0].clone());
    assert!(validate_rubric(&duplicate).is_err());
    let mut oversized = rubric(10.0);
    oversized.criteria[0].description = "x".repeat(4001);
    assert!(validate_rubric(&oversized).is_err());
}
#[test]
fn validates_levels_and_reads_them_back() {
    let directory = tempfile::tempdir().unwrap();
    let db = crate::db::schema::open_with_key(directory.path(), [7; 32]).unwrap();
    let mut with_levels = rubric(4.0);
    with_levels.criteria[0].levels = vec![
        crate::models::PerformanceLevel {
            label: "Excellent".into(),
            score: 4.0,
            description: "Complete".into(),
        },
        crate::models::PerformanceLevel {
            label: "Partial".into(),
            score: 2.0,
            description: String::new(),
        },
    ];
    validate_rubric(&with_levels).unwrap();
    let mut too_high = with_levels.clone();
    too_high.criteria[0].levels[0].score = 4.5;
    assert!(validate_rubric(&too_high).is_err());
    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    let id = insert_rubric(conn, &with_levels, 1).unwrap();
    let json: Option<String> = conn
        .query_row(
            "SELECT levels_json FROM criteria_rubric WHERE rubric_id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(parse_levels(json), with_levels.criteria[0].levels);
}
#[test]
fn deleting_rubrics_and_assignments_respects_existing_work_and_is_audited() {
    let directory = tempfile::tempdir().unwrap();
    let db = crate::db::schema::open_with_key(directory.path(), [8; 32]).unwrap();
    let mut guard = db.conn.lock().unwrap();
    let conn = guard.as_mut().unwrap();
    let used = insert_rubric(conn, &rubric(10.0), 1).unwrap();
    let unused = insert_rubric(conn, &rubric(5.0), 1).unwrap();
    conn.execute(
        "INSERT INTO assignments (rubric_id,text) VALUES (?1,'Task')",
        [used],
    )
    .unwrap();
    let assignment = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO submissions (assignment_id,status_pipeline) VALUES (?1,'ocr_complete')",
        [assignment],
    )
    .unwrap();
    assert!(delete_rubric(conn, used).is_err());
    delete_rubric(conn, unused).unwrap();
    delete_assignment(conn, assignment).unwrap();
    delete_rubric(conn, used).unwrap();
    let remaining: i64 = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM submissions)+(SELECT COUNT(*) FROM rubrics)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(remaining, 0);
    let events: Vec<String> = conn
        .prepare("SELECT event FROM logs_audit ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        events,
        ["rubric_deleted", "assignment_deleted", "rubric_deleted"]
    );
}
