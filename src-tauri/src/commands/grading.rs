use serde::Deserialize;
use tauri::State;

use crate::db::DbState;
use crate::pipeline::inference_client::InferenceClient;
use crate::pipeline::prompt_builder::{
    self, CriterionEvidence, FeedbackAndConsistency, PromptCriterion,
};
use crate::pipeline::redaction;

fn assignment_and_criteria(
    conn: &rusqlite::Connection,
    submission_id: i64,
) -> Result<(String, Vec<PromptCriterion>), String> {
    let (assignment_id,): (i64,) = conn
        .query_row(
            "SELECT assignment_id FROM submissions WHERE id = ?1",
            [submission_id],
            |r| Ok((r.get(0)?,)),
        )
        .map_err(|e| e.to_string())?;
    let (rubric_id, text_assignment): (i64, String) = conn
        .query_row(
            "SELECT rubric_id, text FROM assignments WHERE id = ?1",
            [assignment_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT code, description, score_max FROM criteria_rubric
             WHERE rubric_id = ?1 ORDER BY sort_order",
        )
        .map_err(|e| e.to_string())?;
    let criteria = stmt
        .query_map([rubric_id], |r| {
            Ok(PromptCriterion {
                id: r.get(0)?,
                description: r.get(1)?,
                score_max: r.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok((text_assignment, criteria))
}

fn id_of_criterion(
    conn: &rusqlite::Connection,
    submission_id: i64,
    code: &str,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT cr.id FROM criteria_rubric cr
         JOIN assignments e ON e.rubric_id = cr.rubric_id
         JOIN submissions s ON s.assignment_id = e.id
         WHERE s.id = ?1 AND cr.code = ?2",
        rusqlite::params![submission_id, code],
        |r| r.get(0),
    )
    .map_err(|e| format!("criterion '{code}' not found for the submission {submission_id}: {e}"))
}

/// Validate a score independently of the database. The frontend input's
/// `min` and `max` attributes do not enforce server-side constraints.
fn validate_score(score: f64, score_max: f64) -> Result<(), String> {
    if score.is_finite() && score_max.is_finite() && (0.0..=score_max).contains(&score) {
        Ok(())
    } else {
        Err(format!(
            "score outside of range: {score} (maximum {score_max})"
        ))
    }
}

fn id_and_max_of_criterion(
    conn: &rusqlite::Connection,
    submission_id: i64,
    code: &str,
) -> Result<(i64, f64), String> {
    conn.query_row(
        "SELECT cr.id, cr.score_max FROM criteria_rubric cr
         JOIN assignments e ON e.rubric_id = cr.rubric_id
         JOIN submissions s ON s.assignment_id = e.id
         WHERE s.id = ?1 AND cr.code = ?2",
        rusqlite::params![submission_id, code],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .map_err(|e| format!("criterion '{code}' not found for the submission {submission_id}: {e}"))
}

/// Call A (Milestone 5): locate evidence without assigning a score.
/// `load_confirmed` requires the mandatory redaction review first.
#[tauri::command]
pub async fn cmd_request_evidence(
    db: State<'_, std::sync::Arc<DbState>>,
    submission_id: i64,
) -> Result<Vec<CriterionEvidence>, String> {
    let (text_assignment, criteria, confirmed, settings) = {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("the database is closed")?;
        let (text_assignment, criteria) = assignment_and_criteria(conn, submission_id)?;
        let confirmed =
            redaction::load_confirmed(conn, submission_id).map_err(|e| e.to_string())?;
        (
            text_assignment,
            criteria,
            confirmed,
            super::settings::load(conn)?,
        )
    };

    let client = InferenceClient::new(settings.url.clone()).map_err(|e| e.to_string())?;
    let evidence = prompt_builder::request_evidence(
        &client,
        &settings.model,
        &text_assignment,
        &criteria,
        &confirmed,
    )
    .await
    .map_err(|e| e.to_string())?;

    let stored = evidence.clone();
    crate::tasks::run(db.inner().clone(),move|db| {
        let mut guard = db.conn.lock().map_err(|e| e.to_string())?;
        let connection = guard.as_mut().ok_or("the database is closed")?;
        let tx = connection.transaction().map_err(|e| e.to_string())?;
        let conn = &tx;
        redaction::load_confirmed(conn, submission_id).map_err(|e| e.to_string())?;
        for ev in &stored {
            let criterion_id = id_of_criterion(conn, submission_id, &ev.criterion_id)?;
            conn.execute(
                "INSERT INTO results (submission_id, criterion_id, ai_evidence_json) VALUES (?1, ?2, ?3)
                 ON CONFLICT(submission_id, criterion_id) DO UPDATE SET ai_evidence_json = excluded.ai_evidence_json",
                rusqlite::params![
                    submission_id,
                    criterion_id,
                    serde_json::to_string(&ev.evidence_textual).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        conn.execute(
            "INSERT INTO logs_audit (submission_id, event, actor, model_version, payload_json)
             VALUES (?1, 'ai_evidence_request', 'ai', ?2, ?3)",
            rusqlite::params![
                submission_id,
                settings.model,
                serde_json::json!({"criteria_count":stored.len()}).to_string()
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        db.persist(connection).map_err(|e|e.to_string())?;
        Ok(())
    }).await?;

    Ok(evidence)
}

#[derive(Debug, Deserialize)]
pub struct CriterionAssessment {
    pub criterion_id: String,
    pub score: f64,
    pub comment_teacher: Option<String>,
}

fn save_assessments(
    conn: &mut rusqlite::Connection,
    submission_id: i64,
    expected_revision: i64,
    assessments: &[CriterionAssessment],
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let (status, revision): (String, i64) = tx
        .query_row(
            "SELECT status_pipeline,grade_revision FROM submissions WHERE id=?1",
            [submission_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    if status != "redacted" || revision != expected_revision {
        return Err("the assessment changed; reload before saving".into());
    }
    let (_, criteria) = assignment_and_criteria(&tx, submission_id)?;
    let mut codes = std::collections::HashSet::new();
    if assessments.len() != criteria.len() || assessments.is_empty() {
        return Err("provide one score for every criterion".into());
    }
    for assessment in assessments {
        if !codes.insert(&assessment.criterion_id) {
            return Err("duplicate criterion score".into());
        }
        let (id, max) = id_and_max_of_criterion(&tx, submission_id, &assessment.criterion_id)?;
        validate_score(assessment.score, max)?;
        tx.execute("INSERT INTO results (submission_id,criterion_id,score_final,comment_teacher) VALUES (?1,?2,?3,?4) ON CONFLICT(submission_id,criterion_id) DO UPDATE SET score_final=excluded.score_final,comment_teacher=excluded.comment_teacher",rusqlite::params![submission_id,id,assessment.score,assessment.comment_teacher]).map_err(|e|e.to_string())?;
    }
    tx.execute("UPDATE submissions SET grade_revision=grade_revision+1,feedback_json=NULL,feedback_revision=NULL WHERE id=?1",[submission_id]).map_err(|e|e.to_string())?;
    tx.execute("INSERT INTO logs_audit (submission_id,event,actor,payload_json) VALUES (?1,'assessment_saved','teacher',?2)",rusqlite::params![submission_id,serde_json::json!({"revision":revision+1}).to_string()]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_save_grade_tentative(
    db: State<'_, std::sync::Arc<DbState>>,
    submission_id: i64,
    expected_revision: i64,
    assessments: Vec<CriterionAssessment>,
) -> Result<i64, String> {
    crate::tasks::run(db.inner().clone(), move |db| {
        let mut guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_mut().ok_or("the database is closed")?;
        save_assessments(conn, submission_id, expected_revision, &assessments)?;
        db.persist(conn).map_err(|e| e.to_string())?;
        Ok(expected_revision + 1)
    })
    .await
}

#[derive(serde::Serialize)]
pub struct SavedCriterion {
    criterion_id: String,
    score: Option<f64>,
    comment_teacher: Option<String>,
    evidence_textual: Vec<String>,
}
#[derive(serde::Serialize)]
pub struct GradingState {
    revision: i64,
    criteria: Vec<SavedCriterion>,
    feedback: Option<FeedbackAndConsistency>,
}

#[tauri::command]
pub async fn cmd_load_grading_state(
    db: State<'_, std::sync::Arc<DbState>>,
    submission_id: i64,
) -> Result<GradingState, String> {
    crate::tasks::run(db.inner().clone(),move|db| {
        let guard=db.conn.lock().map_err(|e|e.to_string())?;let conn=guard.as_ref().ok_or("the database is closed")?;
        let (revision,feedback):(i64,Option<String>)=conn.query_row("SELECT grade_revision,CASE WHEN feedback_revision=grade_revision THEN feedback_json END FROM submissions WHERE id=?1",[submission_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?;
        let mut stmt=conn.prepare("SELECT cr.code,r.score_final,r.comment_teacher,r.ai_evidence_json FROM submissions s JOIN assignments a ON a.id=s.assignment_id JOIN criteria_rubric cr ON cr.rubric_id=a.rubric_id LEFT JOIN results r ON r.submission_id=s.id AND r.criterion_id=cr.id WHERE s.id=?1 ORDER BY cr.sort_order").map_err(|e|e.to_string())?;
        let criteria=stmt.query_map([submission_id],|r| {
            let evidence:Option<String>=r.get(3)?;
            Ok(SavedCriterion {criterion_id:r.get(0)?,score:r.get(1)?,comment_teacher:r.get(2)?,evidence_textual:evidence.and_then(|s|serde_json::from_str(&s).ok()).unwrap_or_default()})
        }).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        Ok(GradingState {revision,criteria,feedback:feedback.map(|s|serde_json::from_str(&s)).transpose().map_err(|e|e.to_string())?})
    }).await
}

/// Call B (Milestone 5): draft feedback and flag possible inconsistencies
/// between the teacher's saved scores and the available evidence.
#[tauri::command]
pub async fn cmd_request_feedback(
    db: State<'_, std::sync::Arc<DbState>>,
    submission_id: i64,
) -> Result<FeedbackAndConsistency, String> {
    let (text_assignment, criteria, confirmed, assessment_teacher, revision, settings) = {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("the database is closed")?;
        let (text_assignment, criteria) = assignment_and_criteria(conn, submission_id)?;
        let confirmed =
            redaction::load_confirmed(conn, submission_id).map_err(|e| e.to_string())?;

        let mut stmt = conn
            .prepare(
                "SELECT cr.code, r.score_final FROM results r
                 JOIN criteria_rubric cr ON cr.id = r.criterion_id
                 WHERE r.submission_id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let assessment_teacher: Vec<(String, Option<f64>)> = stmt
            .query_map([submission_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        if assessment_teacher.len() != criteria.len()
            || assessment_teacher.iter().any(|(_, score)| score.is_none())
        {
            return Err("save a score for every criterion before requesting feedback".into());
        }

        let revision: i64 = conn
            .query_row(
                "SELECT grade_revision FROM submissions WHERE id=?1",
                [submission_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        (
            text_assignment,
            criteria,
            confirmed,
            assessment_teacher,
            revision,
            super::settings::load(conn)?,
        )
    };

    let client = InferenceClient::new(settings.url.clone()).map_err(|e| e.to_string())?;
    let feedback = prompt_builder::request_feedback(
        &client,
        &settings.model,
        &text_assignment,
        &criteria,
        &confirmed,
        &assessment_teacher,
    )
    .await
    .map_err(|e| e.to_string())?;

    let stored = feedback.clone();
    crate::tasks::run(db.inner().clone(),move|db| {
        let mut guard=db.conn.lock().map_err(|e|e.to_string())?;
        let conn=guard.as_mut().ok_or("the database is closed")?;
        let tx=conn.transaction().map_err(|e|e.to_string())?;
        persist_feedback(&tx,submission_id,revision,&stored)?;
        tx.execute("INSERT INTO logs_audit (submission_id,event,actor,model_version,payload_json) VALUES (?1,'ai_feedback_request','ai',?2,?3)",rusqlite::params![submission_id,settings.model,serde_json::json!({"revision":revision,"inconsistency_count":stored.inconsistencies.len()}).to_string()]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e|e.to_string())?;
        db.persist(conn).map_err(|e|e.to_string())?;
        Ok(())
    }).await?;

    Ok(feedback)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assessment_database() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../db/migrations/0001_init.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../db/migrations/0003_workflow.sql"))
            .unwrap();
        conn.execute_batch("INSERT INTO rubrics (id,title,subject,grade_level,content_json) VALUES (1,'t','s','g','{}'); INSERT INTO criteria_rubric (id,rubric_id,code,description,score_max,sort_order) VALUES (1,1,'C1','One',10,0),(2,1,'C2','Two',10,1); INSERT INTO assignments (id,rubric_id,text) VALUES (1,1,'assignment'); INSERT INTO submissions (id,assignment_id,status_pipeline) VALUES (1,1,'redacted');").unwrap();
        conn
    }
    fn assessments(second: f64) -> Vec<CriterionAssessment> {
        vec![
            CriterionAssessment {
                criterion_id: "C1".into(),
                score: 4.0,
                comment_teacher: None,
            },
            CriterionAssessment {
                criterion_id: "C2".into(),
                score: second,
                comment_teacher: None,
            },
        ]
    }
    #[test]
    fn rejected_score_batch_rolls_back_and_does_not_advance_revision() {
        let mut conn = assessment_database();
        assert!(save_assessments(&mut conn, 1, 0, &assessments(11.0)).is_err());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM results", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        save_assessments(&mut conn, 1, 0, &assessments(5.0)).unwrap();
        assert!(save_assessments(&mut conn, 1, 0, &assessments(6.0)).is_err());
        let score: f64 = conn
            .query_row(
                "SELECT score_final FROM results WHERE criterion_id=2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(score, 5.0);
    }
    #[test]
    fn rejects_and_invalidates_feedback_for_obsolete_scores() {
        let mut conn = assessment_database();
        save_assessments(&mut conn, 1, 0, &assessments(5.0)).unwrap();
        let feedback = FeedbackAndConsistency {
            comment_feedback: "Draft".into(),
            inconsistencies: vec![],
        };
        assert!(persist_feedback(&conn, 1, 0, &feedback).is_err());
        persist_feedback(&conn, 1, 1, &feedback).unwrap();
        save_assessments(&mut conn, 1, 1, &assessments(6.0)).unwrap();
        let json: Option<String> = conn
            .query_row(
                "SELECT feedback_json FROM submissions WHERE id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(json.is_none());
        assert!(persist_feedback(&conn, 1, 1, &feedback).is_err());
    }

    #[test]
    fn validate_score_accepts_range_valid() {
        assert!(validate_score(0.0, 10.0).is_ok());
        assert!(validate_score(10.0, 10.0).is_ok());
        assert!(validate_score(4.5, 10.0).is_ok());
    }

    #[test]
    fn validate_score_rejects_outside_of_range() {
        assert!(validate_score(-1.0, 10.0).is_err());
        assert!(validate_score(10.1, 10.0).is_err());
    }
}

fn persist_feedback(
    conn: &rusqlite::Connection,
    id: i64,
    revision: i64,
    feedback: &FeedbackAndConsistency,
) -> Result<(), String> {
    let json = serde_json::to_string(feedback).map_err(|e| e.to_string())?;
    let changed=conn.execute("UPDATE submissions SET feedback_json=?1,feedback_revision=?2 WHERE id=?3 AND status_pipeline='redacted' AND grade_revision=?2",rusqlite::params![json,revision,id]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err(
            "assessment changed while feedback was generated; request feedback again".into(),
        );
    }
    Ok(())
}
