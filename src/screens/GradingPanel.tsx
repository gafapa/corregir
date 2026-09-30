import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { CriterionWithId, SubmissionSummary, CriterionEvidence, FeedbackAndConsistency, GradingState } from "../lib/types";

/**
 * Milestone 5, grading assistant mode: two AI calls separated by the
 * teacher's own assessment. The AI locates evidence first, then drafts
 * feedback and flags inconsistencies. Requires confirmed redaction.
 */
export function GradingPanel({
  submission,
  criteria,
  onGradeConfirmed,
}: {
  submission: SubmissionSummary;
  criteria: CriterionWithId[];
  onGradeConfirmed?: () => void;
}) {
  const [evidenceList, setEvidenceList] = useState<CriterionEvidence[]>([]);
  const [scores, setScores] = useState<Record<string, string>>({});
  const [feedback, setFeedback] = useState<FeedbackAndConsistency | null>(null);
  const [confirmedGrade, setConfirmedGrade] = useState<number | null>(null);
  const [loading, setLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const [restoreVersion, setRestoreVersion] = useState(0);
  const [comments, setComments] = useState<Record<string, string>>({});
  const [assessmentSaved, setAssessmentSaved] = useState(false);

  useEffect(() => {
    let active = true;
    setLoading("restore");
    setError(null);
    invoke<GradingState>("cmd_load_grading_state", { submissionId: submission.id }).then((state) => {
      if (!active) return;
      setRevision(state.revision);
      setScores(Object.fromEntries(state.criteria.filter((item) => item.score !== null).map((item) => [item.criterion_id, String(item.score)])));
      setComments(Object.fromEntries(state.criteria.map((item) => [item.criterion_id, item.comment_teacher ?? ""])));
      setEvidenceList(state.criteria.map((item) => ({ criterion_id: item.criterion_id, evidence_textual: item.evidence_textual })));
      setFeedback(state.feedback);
      setAssessmentSaved(state.criteria.length > 0 && state.criteria.every((item) => item.score !== null));
    }).catch((error) => { if (active) setError(String(error)); }).finally(() => { if (active) setLoading(null); });
    return () => { active = false; };
  }, [submission.id, submission.status_pipeline, restoreVersion]);

  async function exportPdf() {
    setError(null);
    try {
      const destination = await save({
        defaultPath: `feedback-submission-${submission.id}.pdf`,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (!destination) return;
      await invoke("cmd_export_pdf_feedback", { submissionId: submission.id, destinationPath: destination });
      alert(`Feedback sheet saved to ${destination}`);
    } catch (e) {
      setError(String(e));
    }
  }

  if (submission.status_pipeline === "grade_confirmed") {
    return (
      <div>
        <p>
          ✅ Grade confirmed for submission #{submission.id}
          {submission.student_name ? ` (${submission.student_name})` : ""}.
        </p>
        <button onClick={exportPdf}>Export feedback sheet (PDF)</button>
        {error && <pre role="alert" style={{ color: "crimson" }}>{error}</pre>}
      </div>
    );
  }
  if (submission.status_pipeline !== "redacted") {
    return null;
  }

  async function requestEvidence() {
    setLoading("evidence");
    setError(null);
    try {
      const result = await invoke<CriterionEvidence[]>("cmd_request_evidence", {
        submissionId: submission.id,
      });
      setEvidenceList(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(null);
    }
  }

  async function saveTentativeGrade() {
    setLoading("grade");
    setError(null);
    try {
      const assessments = criteria
        .filter((c) => scores[c.code] !== undefined && scores[c.code] !== "")
        .map((c) => ({
          // Nested field names must match Rust `CriterionAssessment` exactly.
          // Tauri converts top-level command arguments, but serde does not
          // convert fields inside this object from snake_case to camelCase.
          criterion_id: c.code,
          score: Number(scores[c.code]),
          comment_teacher: comments[c.code]?.trim() || null,
        }));
      if (assessments.length !== criteria.length || assessments.length === 0) {
        throw new Error("Enter a score for every criterion before saving.");
      }
      const nextRevision = await invoke<number>("cmd_save_grade_tentative", { submissionId: submission.id, expectedRevision: revision, assessments });
      setRevision(nextRevision);
      setFeedback(null);
      setAssessmentSaved(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(null);
    }
  }

  async function confirmGrade() {
    setLoading("confirm");
    setError(null);
    try {
      const total = await invoke<number>("cmd_confirm_grade", { submissionId: submission.id });
      setConfirmedGrade(total);
      onGradeConfirmed?.();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(null);
    }
  }

  async function requestFeedback() {
    setLoading("feedback");
    setError(null);
    try {
      const result = await invoke<FeedbackAndConsistency>("cmd_request_feedback", {
        submissionId: submission.id,
      });
      setFeedback(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(null);
    }
  }

  return (
    <div style={{ border: "1px solid #99c", padding: "0.5rem", marginTop: "0.5rem" }}>
      <p role="status" aria-live="polite">{loading ? "Working…" : assessmentSaved ? "Assessment saved." : "Save changes before confirming."}</p>
      <h4>
        Grading — assistant mode
        {submission.student_name ? ` — ${submission.student_name}` : ""}
      </h4>

      <button onClick={() => setRestoreVersion((version) => version + 1)} disabled={loading !== null}>Reload saved assessment</button>
      <button onClick={requestEvidence} disabled={loading !== null}>
        {loading === "evidence" ? "Asking AI…" : "1. Find evidence (no grade)"}
      </button>

      {criteria.map((c) => {
        const evidence = evidenceList.find((item) => item.criterion_id === c.code);
        return (
          <div key={c.id} style={{ marginTop: "0.5rem" }}>
            <strong>
              {c.code} ({c.score_max} pts): {c.description}
            </strong>
            {evidence && (
              <ul>
                {evidence.evidence_textual.map((quote, i) => (
                  <li key={i}>
                    <em>"{quote}"</em>
                  </li>
                ))}
              </ul>
            )}
            <label>
              Your score for {c.code}:{" "}
              <input
                disabled={loading !== null}
                type="number"
                min={0}
                max={c.score_max}
                step={0.5}
                value={scores[c.code] ?? ""}
                onChange={(e) => {
                  const value = e.currentTarget.value;
                  setScores((prev) => ({ ...prev, [c.code]: value }));
                  setAssessmentSaved(false);
                  setFeedback(null);
                }}
              />
            </label>
            <label>
              Teacher comment for {c.code}:
              <textarea disabled={loading !== null} value={comments[c.code] ?? ""} onChange={(event) => {
                const value = event.currentTarget.value;
                setComments((previous) => ({ ...previous, [c.code]: value }));
                setAssessmentSaved(false);
                setFeedback(null);
              }} />
            </label>
          </div>
        );
      })}

      <div style={{ marginTop: "0.5rem" }}>
        <button onClick={saveTentativeGrade} disabled={loading !== null}>
          {loading === "grade" ? "Saving…" : "2. Save my assessment"}
        </button>
        <button onClick={requestFeedback} disabled={loading !== null || !assessmentSaved}>
          {loading === "feedback" ? "Asking AI…" : "3. Generate feedback and check inconsistencies"}
        </button>
      </div>

      {feedback && (
        <div style={{ marginTop: "0.5rem" }}>
          <p>
            <strong>Feedback for the student:</strong> {feedback.comment_feedback}
          </p>
          {feedback.inconsistencies.length > 0 && (
            <>
              <strong style={{ color: "darkorange" }}>Possible inconsistencies:</strong>
              <ul>
                {feedback.inconsistencies.map((inc, i) => (
                  <li key={i}>
                    [{inc.criterion_id}] {inc.observation}
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}

      <div style={{ marginTop: "0.5rem" }}>
        <button onClick={confirmGrade} disabled={loading !== null || !assessmentSaved}>
          {loading === "confirm" ? "Confirming…" : "4. Confirm grade (Milestone 6)"}
        </button>
        {confirmedGrade !== null && (
          <span style={{ marginLeft: "0.5rem" }}>
            ✅ Confirmed total score: <strong>{confirmedGrade}</strong>
          </span>
        )}
      </div>

      {error && <pre role="alert" style={{ color: "crimson" }}>{error}</pre>}
    </div>
  );
}
