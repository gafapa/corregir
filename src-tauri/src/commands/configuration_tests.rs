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
