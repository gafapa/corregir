export interface PerformanceLevel {
  label: string;
  score: number;
  description: string;
}

export interface NewCriterion {
  code: string;
  description: string;
  score_max: number;
  levels?: PerformanceLevel[];
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

export type SubmissionStatus = "ocr_complete" | "redacted" | "grade_confirmed";

export interface SubmissionSummary {
  id: number;
  assignment_id: number;
  text_ocr: string | null;
  method_ocr: string | null;
  status_pipeline: string;
  student_name: string | null;
  student_id?: number | null;
  source_name?: string | null;
  identities_purged?: boolean;
  has_evidence?: boolean;
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

export interface EvidenceResult {
  evidence: CriterionEvidence[];
  discarded_quotes: number;
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

export interface Student {
  id: number;
  full_name: string;
}

export interface ClassRoster {
  id: number;
  name: string;
  students: Student[];
}

/** Synthetic test cases described in the project plan. */
export const SYNTHETIC_RUBRICS: NewRubric[] = [
  {
    title: "Poem analysis",
    subject: "Language and literature",
    grade_level: "Year 8",
    criteria: [
      {
        code: "C1",
        description: "Correctly identifies the main theme.",
        score_max: 4,
        levels: [
          { label: "Complete", score: 4, description: "States the theme and supports it with the text." },
          { label: "Partial", score: 2, description: "States the theme without support." },
          { label: "Not achieved", score: 0, description: "The theme is missing or incorrect." },
        ],
      },
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
  title?: string | null;
  text: string;
  class_id?: number | null;
  submission_count: number;
  redaction_pending?: number;
  ready_to_grade?: number;
  confirmed?: number;
  identified?: number;
  last_confirmed_at?: string | null;
}

export interface GradingState {
  revision: number;
  criteria: { criterion_id: string; score: number | null; comment_teacher: string | null; evidence_textual: string[] }[];
  feedback: FeedbackAndConsistency | null;
  feedback_final?: string | null;
  feedback_ai_assisted?: boolean;
}

export interface ImportProgress {
  request_id: string;
  page: number;
  total: number;
  stage: string;
}

export interface Preferences {
  feedback_language: "auto" | "gl" | "es" | "en";
  retention_days: number | null;
}

export interface RetentionDue {
  assignment_id: number;
  title: string | null;
  text: string;
  identified: number;
  last_confirmed_at: string | null;
}

export interface KnownStudent {
  name: string;
  submissions: number;
  roster_entries: number;
}

/** Short label for an assignment: its title, or the first line of its instructions. */
export function assignmentLabel(assignment: Pick<AssignmentSummary, "id" | "title" | "text">): string {
  const title = assignment.title?.trim();
  if (title) return title;
  const first = assignment.text.split("\n").find(line => line.trim())?.trim() ?? "";
  return first.length > 60 ? `${first.slice(0, 60)}…` : first || `#${assignment.id}`;
}
