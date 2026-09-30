export interface NewCriterion {
  code: string;
  description: string;
  score_max: number;
}

export interface NewRubric {
  title: string;
  subject: string;
  grade_level: string;
  criteria: NewCriterion[];
}

export interface CriterionWithId extends NewCriterion {
  id: number;
  sort_order: number;
}

export interface RubricWithCriteria {
  id: number;
  title: string;
  subject: string;
  grade_level: string;
  version: number;
  criteria: CriterionWithId[];
}

export interface SubmissionSummary {
  id: number;
  assignment_id: number;
  text_ocr: string | null;
  method_ocr: string | null;
  status_pipeline: string;
  student_name: string | null;
}

export interface IdentifierCandidate {
  kind: string;
  text: string;
  start: number;
  end: number;
}

export interface CriterionEvidence {
  criterion_id: string;
  evidence_textual: string[];
}

export interface Inconsistency {
  criterion_id: string;
  observation: string;
}

export interface FeedbackAndConsistency {
  comment_feedback: string;
  inconsistencies: Inconsistency[];
}

export interface AuditLog {
  id: number;
  submission_id: number | null;
  event: string;
  actor: string;
  model_version: string | null;
  payload_json: string | null;
  created_at: string;
}

/** Synthetic test cases described in the project plan. */
export const SYNTHETIC_RUBRICS: NewRubric[] = [
  {
    title: "Poem analysis",
    subject: "Language and literature",
    grade_level: "Year 8",
    criteria: [
      { code: "C1", description: "Correctly identifies the main theme.", score_max: 4 },
      {
        code: "C2",
        description: "Identifies and explains at least two literary devices with textual examples.",
        score_max: 3,
      },
      { code: "C3", description: "Coherence and clarity of expression.", score_max: 3 },
    ],
  },
  {
    title: "Constant acceleration: speed and distance",
    subject: "Physics and chemistry",
    grade_level: "Year 10",
    criteria: [
      { code: "C1", description: "Selects the correct constant acceleration formulas.", score_max: 3 },
      { code: "C2", description: "Substitutes values and calculates without arithmetic errors.", score_max: 4 },
      { code: "C3", description: "Uses correct units and interprets the result.", score_max: 3 },
    ],
  },
];

export interface AssignmentSummary {
  id: number;
  rubric_id: number;
  text: string;
  submission_count: number;
}

export interface GradingState {
  revision: number;
  criteria: { criterion_id: string; score: number | null; comment_teacher: string | null; evidence_textual: string[] }[];
  feedback: FeedbackAndConsistency | null;
}

export interface ImportProgress {
  request_id: string;
  page: number;
  total: number;
  stage: string;
}
