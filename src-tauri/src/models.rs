use serde::{Deserialize, Serialize};

/// Optional rubric descriptor: selecting a level fills in its score.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PerformanceLevel {
    pub label: String,
    pub score: f64,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCriterion {
    pub code: String,
    pub description: String,
    pub score_max: f64,
    #[serde(default)]
    pub levels: Vec<PerformanceLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRubric {
    pub title: String,
    pub subject: String,
    pub grade_level: String,
    pub criteria: Vec<NewCriterion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionWithId {
    pub id: i64,
    pub code: String,
    pub description: String,
    pub score_max: f64,
    pub sort_order: i64,
    pub levels: Vec<PerformanceLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricWithCriteria {
    pub id: i64,
    pub title: String,
    pub subject: String,
    pub grade_level: String,
    pub version: i64,
    pub criteria: Vec<CriterionWithId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionSummary {
    pub id: i64,
    pub assignment_id: i64,
    pub text_ocr: Option<String>,
    pub method_ocr: Option<String>,
    pub status_pipeline: String,
    /// Resolved only for this response by joining `alias_student_map`.
    /// The teacher needs to identify submissions during review; this field
    /// does not create another persistent copy of the name.
    pub student_name: Option<String>,
    pub student_id: Option<i64>,
    /// File name selected at import, kept to tell submissions apart.
    pub source_name: Option<String>,
    pub identities_purged: bool,
    pub has_evidence: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: i64,
    pub submission_id: Option<i64>,
    pub event: String,
    pub actor: String,
    pub model_version: Option<String>,
    pub payload_json: Option<String>,
    pub created_at: String,
}
