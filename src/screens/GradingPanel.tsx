import { useEffect, useRef, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { CriterionWithId, SubmissionSummary, CriterionEvidence, EvidenceResult, FeedbackAndConsistency, GradingState } from "../lib/types";
import { useI18n } from "../lib/i18n";

/**
 * Grading assistant mode: two AI calls separated by the teacher's own
 * assessment. The AI locates evidence first, then drafts feedback and flags
 * inconsistencies. The teacher edits and saves the feedback that reports use.
 * Requires confirmed redaction.
 */
export function GradingPanel({
  submission,
  criteria,
  onGradeConfirmed,
  onChanged,
}: {
  submission: SubmissionSummary;
  criteria: CriterionWithId[];
  onGradeConfirmed?: () => void;
  onChanged?: () => void;
}) {
  const { t, language, formatNumber, errorText } = useI18n();
  const [evidenceList, setEvidenceList] = useState<CriterionEvidence[]>([]);
  const [discarded, setDiscarded] = useState(0);
  const [scores, setScores] = useState<Record<string, string>>({});
  const [draft, setDraft] = useState<FeedbackAndConsistency | null>(null);
  const [feedbackText, setFeedbackText] = useState("");
  const [savedFeedback, setSavedFeedback] = useState("");
  const [aiAssisted, setAiAssisted] = useState(false);
  const [loading, setLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const [restoreVersion, setRestoreVersion] = useState(0);
  const [comments, setComments] = useState<Record<string, string>>({});
  const [assessmentSaved, setAssessmentSaved] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [reopenReason, setReopenReason] = useState("");
  const [reopening, setReopening] = useState(false);
  const inferenceId = useRef<string | null>(null);
  const [inferenceStage, setInferenceStage] = useState<string | null>(null);
  useEffect(() => () => {
    if (inferenceId.current) void invoke("cmd_cancel_inference", { requestId: inferenceId.current }).catch(() => {});
  }, []);
  async function askModel<T>(command: string): Promise<T> {
    const requestId = crypto.randomUUID();
    const onProgress = new Channel<{ request_id: string; stage: string }>();
    inferenceId.current = requestId;
    setInferenceStage("queued");
    onProgress.onmessage = message => { if (inferenceId.current === message.request_id) setInferenceStage(message.stage); };
    try { return await invoke<T>(command, { submissionId: submission.id, requestId, onProgress }); }
    finally { inferenceId.current = null; setInferenceStage(null); }
  }
  async function cancelInference() {
    if (!inferenceId.current) return;
    try { await invoke("cmd_cancel_inference", { requestId: inferenceId.current }); }
    catch (cause) { setError(errorText(cause)); }
  }

  useEffect(() => {
    let active = true;
    setLoading("restore");
    setError(null);
    setAssessmentSaved(false);
    invoke<GradingState>("cmd_load_grading_state", { submissionId: submission.id }).then(state => {
      if (!active) return;
      setRevision(state.revision);
      setScores(Object.fromEntries(state.criteria.filter(item => item.score !== null).map(item => [item.criterion_id, String(item.score)])));
      setComments(Object.fromEntries(state.criteria.map(item => [item.criterion_id, item.comment_teacher ?? ""])));
      setEvidenceList(state.criteria.map(item => ({ criterion_id: item.criterion_id, evidence_textual: item.evidence_textual })));
      setDraft(state.feedback);
      const saved = state.feedback_final ?? "";
      setSavedFeedback(saved);
      // An unsaved current AI draft is shown for editing; it is saved only with the teacher's action.
      const text = saved || state.feedback?.comment_feedback || "";
      setFeedbackText(text);
      setAiAssisted(saved ? Boolean(state.feedback_ai_assisted) : Boolean(text));
      setAssessmentSaved(state.criteria.length > 0 && state.criteria.every(item => item.score !== null));
    }).catch(cause => { if (active) setError(errorText(cause)); }).finally(() => { if (active) setLoading(null); });
    return () => { active = false; };
  }, [submission.id, submission.status_pipeline, restoreVersion]);

  const total = criteria.reduce((sum, criterion) => sum + (Number(scores[criterion.code]) || 0), 0);
  const maximum = criteria.reduce((sum, criterion) => sum + criterion.score_max, 0);
  const complete = criteria.length > 0 && criteria.every(criterion => scores[criterion.code] !== undefined && scores[criterion.code] !== "");
  const outOfTen = maximum > 0 ? total / maximum * 10 : 0;
  const feedbackDirty = feedbackText.trim() !== savedFeedback.trim();

  async function exportPdf() {
    setError(null);
    try {
      const destination = await save({
        defaultPath: `${t("export.pdfName")}-${(submission.student_name ?? String(submission.id)).replace(/[\\/:*?"<>|]/g, "_")}.pdf`,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (!destination) return;
      await invoke("cmd_export_pdf_feedback", { submissionId: submission.id, destinationPath: destination, language });
      setNotice(t("export.pdfDone", { path: destination }));
    } catch (cause) {
      setError(errorText(cause));
    }
  }

  async function reopenGrade() {
    setLoading("reopen"); setError(null);
    try {
      await invoke("cmd_reopen_grade", { submissionId: submission.id, reason: reopenReason });
      setReopening(false); setReopenReason("");
      onGradeConfirmed?.();
    } catch (cause) { setError(errorText(cause)); } finally { setLoading(null); }
  }

  if (submission.status_pipeline === "grade_confirmed") {
    return (
      <div className="confirmed-assessment">
        <p>{t("grading.confirmedSummary", { total: formatNumber(total), maximum: formatNumber(maximum), grade: formatNumber(outOfTen) })}</p>
        {savedFeedback && <div className="feedback-panel"><strong>{t("grading.feedbackTitle")}</strong><p>{savedFeedback}</p>{aiAssisted && <p className="supporting-text">{t("grading.aiAssistedNote")}</p>}</div>}
        <div className="action-row">
          <button onClick={() => void exportPdf()}>{t("export.pdfButton")}</button>
          {!submission.identities_purged && <button className="button-quiet" disabled={loading !== null} onClick={() => setReopening(value => !value)}>{t("grading.reopen")}</button>}
        </div>
        {reopening && <div className="reopen-form">
          <label>{t("grading.reopenReason")}<input maxLength={500} value={reopenReason} onChange={event => setReopenReason(event.currentTarget.value)} /></label>
          <div className="action-row"><button className="button-primary" disabled={!reopenReason.trim() || loading !== null} onClick={() => void reopenGrade()}>{t("grading.reopenConfirm")}</button><button onClick={() => setReopening(false)}>{t("common.cancel")}</button></div>
        </div>}
        {notice && <p className="notice" role="status">{notice}</p>}
        {error && <pre role="alert">{error}</pre>}
      </div>
    );
  }
  if (submission.status_pipeline !== "redacted") {
    return null;
  }

  async function requestEvidence() {
    setLoading("evidence");
    setError(null); setNotice(null);
    try {
      const result = await askModel<EvidenceResult>("cmd_request_evidence");
      setEvidenceList(result.evidence);
      setDiscarded(result.discarded_quotes);
      onChanged?.();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setLoading(null);
    }
  }

  async function saveTentativeGrade() {
    setLoading("grade");
    setError(null);
    try {
      const assessments = criteria
        .filter(c => scores[c.code] !== undefined && scores[c.code] !== "")
        .map(c => ({
          // Nested field names must match Rust `CriterionAssessment` exactly.
          // Tauri converts top-level command arguments, but serde does not
          // convert fields inside this object from snake_case to camelCase.
          criterion_id: c.code,
          score: Number(scores[c.code]),
          comment_teacher: comments[c.code]?.trim() || null,
        }));
      if (assessments.length !== criteria.length || assessments.length === 0) {
        throw new Error(t("grading.scoreEvery"));
      }
      const nextRevision = await invoke<number>("cmd_save_grade_tentative", { submissionId: submission.id, expectedRevision: revision, assessments });
      setRevision(nextRevision);
      setDraft(null);
      setAssessmentSaved(true);
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setLoading(null);
    }
  }

  async function saveFeedback(): Promise<boolean> {
    setLoading("feedback-save"); setError(null);
    try {
      await invoke("cmd_save_feedback", { submissionId: submission.id, text: feedbackText, aiAssisted });
      setSavedFeedback(feedbackText.trim());
      setFeedbackText(feedbackText.trim());
      return true;
    } catch (cause) { setError(errorText(cause)); return false; } finally { setLoading(null); }
  }

  async function confirmGrade() {
    setLoading("confirm");
    setError(null);
    try {
      const confirmedTotal = await invoke<number>("cmd_confirm_grade", { submissionId: submission.id, expectedRevision: revision });
      setNotice(t("grading.confirmedTotal", { total: formatNumber(confirmedTotal) }));
      onGradeConfirmed?.();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setLoading(null);
    }
  }

  async function requestFeedback() {
    setLoading("feedback");
    setError(null);
    try {
      const result = await askModel<FeedbackAndConsistency>("cmd_request_feedback");
      setDraft(result);
      if (!feedbackText.trim()) {
        setFeedbackText(result.comment_feedback);
        setAiAssisted(true);
      }
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setLoading(null);
    }
  }

  function updateScore(code: string, value: string) {
    setScores(previous => ({ ...previous, [code]: value }));
    setAssessmentSaved(false);
    setDraft(null);
  }

  return (
    <div className="grading-panel">
      <h4>{t("grading.title")}</h4>
      <p className="assessment-status" role="status" aria-live="polite">{loading ? t("common.working") : assessmentSaved ? t("grading.saved") : t("grading.unsaved")}</p>
      {inferenceStage && <div className="action-row"><p role="status">{inferenceStage === "queued" ? t("ai.queued") : inferenceStage === "saving" ? t("ai.saving") : t("ai.running")}</p><button onClick={() => void cancelInference()} disabled={inferenceStage === "saving"}>{t("ai.cancel")}</button></div>}

      <div className="action-row">
        <button className="button-quiet" onClick={() => setRestoreVersion(version => version + 1)} disabled={loading !== null}>{t("grading.reload")}</button>
        <button onClick={() => void requestEvidence()} disabled={loading !== null}>
          {loading === "evidence" ? t("ai.asking") : t("grading.findEvidence")}
        </button>
      </div>
      {discarded > 0 && <p className="supporting-text" role="status">{t("grading.discardedQuotes", { count: discarded })}</p>}

      {criteria.map(c => {
        const evidence = evidenceList.find(item => item.criterion_id === c.code);
        const levels = c.levels ?? [];
        return (
          <div key={c.id} className="criterion">
            <div className="criterion-title">
              <span className="criterion-code">{c.code} · {t("grading.points", { points: formatNumber(c.score_max) })}</span>
              <strong>{c.description}</strong>
            </div>
            {evidence && evidence.evidence_textual.length > 0 && (
              <ul className="evidence-list" aria-label={t("grading.evidenceFor", { code: c.code })}>
                {evidence.evidence_textual.map((quote, i) => (
                  <li key={i}>
                    <em>"{quote}"</em>
                  </li>
                ))}
              </ul>
            )}
            {evidence && evidence.evidence_textual.length === 0 && <p className="supporting-text">{t("grading.noEvidence")}</p>}
            {levels.length > 0 && <div className="level-buttons" role="group" aria-label={t("grading.levelsFor", { code: c.code })}>
              {levels.map(level => <button key={`${level.label}-${level.score}`} type="button" title={level.description} aria-pressed={scores[c.code] !== undefined && scores[c.code] !== "" && Number(scores[c.code]) === level.score} disabled={loading !== null} onClick={() => updateScore(c.code, String(level.score))}>
                {level.label} · {formatNumber(level.score)}
              </button>)}
            </div>}
            <div className="criterion-fields">
            <label>
              {t("grading.scoreFor", { code: c.code })}{" "}
              <input
                disabled={loading !== null}
                type="number"
                min={0}
                max={c.score_max}
                step={0.25}
                value={scores[c.code] ?? ""}
                onChange={event => updateScore(c.code, event.currentTarget.value)}
              />
            </label>
            <label>
              {t("grading.commentFor", { code: c.code })}
              <textarea disabled={loading !== null} value={comments[c.code] ?? ""} onChange={event => {
                const value = event.currentTarget.value;
                setComments(previous => ({ ...previous, [c.code]: value }));
                setAssessmentSaved(false);
                setDraft(null);
              }} />
            </label>
            </div>
          </div>
        );
      })}

      <p className="score-total" aria-live="polite">{complete ? t("grading.total", { total: formatNumber(total), maximum: formatNumber(maximum), grade: formatNumber(outOfTen) }) : t("grading.totalIncomplete", { total: formatNumber(total), maximum: formatNumber(maximum) })}</p>
      <p className="supporting-text">{t("grading.commentsPrivacy")}</p>

      <div className="action-row">
        <button className={assessmentSaved ? "" : "button-primary"} onClick={() => void saveTentativeGrade()} disabled={loading !== null}>
          {loading === "grade" ? t("common.saving") : t("grading.save")}
        </button>
        <button onClick={() => void requestFeedback()} disabled={loading !== null || !assessmentSaved}>
          {loading === "feedback" ? t("ai.asking") : t("grading.generateFeedback")}
        </button>
      </div>

      {draft && draft.inconsistencies.length > 0 && (
        <div className="feedback-panel">
          <strong className="warning-text">{t("grading.inconsistencies")}</strong>
          <ul>
            {draft.inconsistencies.map((inc, i) => (
              <li key={i}>[{inc.criterion_id}] {inc.observation}</li>
            ))}
          </ul>
        </div>
      )}
      <div className="feedback-editor">
        <label>{t("grading.feedbackLabel")}
          <textarea rows={6} maxLength={20000} disabled={loading !== null} value={feedbackText} onChange={event => setFeedbackText(event.currentTarget.value)} placeholder={t("grading.feedbackPlaceholder")} />
        </label>
        {draft && feedbackText.trim() !== draft.comment_feedback.trim() && <div className="action-row compact">
          <button className="button-quiet" disabled={loading !== null} onClick={() => { setFeedbackText(draft.comment_feedback); setAiAssisted(true); }}>{t("grading.useDraft")}</button>
        </div>}
        {aiAssisted && feedbackText.trim() && <p className="supporting-text">{t("grading.aiAssistedNote")}</p>}
        <div className="action-row compact">
          <button disabled={loading !== null || !feedbackDirty} onClick={() => void saveFeedback()}>{t("grading.saveFeedback")}</button>
          {feedbackDirty && <span className="warning-text">{t("grading.feedbackUnsaved")}</span>}
        </div>
      </div>

      <div className="action-row">
        <button className={assessmentSaved ? "button-primary" : ""} onClick={() => void (feedbackDirty ? saveFeedback().then(saved => { if (saved) void confirmGrade(); }) : confirmGrade())} disabled={loading !== null || !assessmentSaved}>
          {loading === "confirm" ? t("grading.confirming") : t("grading.confirm")}
        </button>
      </div>
      {notice && <p className="notice" role="status">{notice}</p>}
      {error && <pre role="alert">{error}</pre>}
    </div>
  );
}
