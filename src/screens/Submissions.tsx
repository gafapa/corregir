import { useEffect, useRef, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { AssignmentSummary, ImportProgress, SubmissionSummary, RubricWithCriteria } from "../lib/types";
import { RedactionPanel } from "./RedactionPanel";
import { GradingPanel } from "./GradingPanel";

export function Submissions() {
  const [rubrics, setRubrics] = useState<RubricWithCriteria[]>([]);
  const [assignments, setAssignments] = useState<AssignmentSummary[]>([]);
  const [rubricId, setRubricId] = useState<number | null>(null);
  const [assignmentId, setAssignmentId] = useState<number | null>(null);
  const [assignmentText, setAssignmentText] = useState("Test assignment for the selected rubric.");
  const [submissions, setSubmissions] = useState<SubmissionSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [importing, setImporting] = useState(false);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const currentAssignment = useRef<number | null>(null);
  const requestId = useRef<string | null>(null);

  function selectAssignment(id: number | null, list = assignments) {
    const assignment = list.find((item) => item.id === id);
    currentAssignment.current = assignment?.id ?? null;
    setAssignmentId(assignment?.id ?? null);
    if (assignment) setRubricId(assignment.rubric_id);
    setSubmissions([]);
    try {
      if (assignment) localStorage.setItem("corregir.activeAssignmentId", String(assignment.id));
      else localStorage.removeItem("corregir.activeAssignmentId");
    } catch { /* Restoring the encrypted workflow does not require local storage. */ }
  }

  useEffect(() => {
    let active = true;
    setLoading(true);
    Promise.all([
      invoke<RubricWithCriteria[]>("cmd_list_rubrics"),
      invoke<AssignmentSummary[]>("cmd_list_assignments"),
    ]).then(([rubricList, assignmentList]) => {
      if (!active) return;
      setRubrics(rubricList);
      setAssignments(assignmentList);
      let savedId = 0;
      try { savedId = Number(localStorage.getItem("corregir.activeAssignmentId")); } catch { /* Use the newest assignment. */ }
      const selected = assignmentList.find((item) => item.id === savedId) ?? assignmentList[0];
      if (selected) selectAssignment(selected.id, assignmentList);
    }).catch((error) => { if (active) setError(String(error)); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    if (assignmentId === null) return;
    let active = true;
    setLoading(true);
    invoke<SubmissionSummary[]>("cmd_list_submissions", { assignmentId }).then((list) => {
      if (active) setSubmissions(list);
    }).catch((error) => { if (active) setError(String(error)); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [assignmentId]);

  async function reloadSubmissions(id: number) {
    try {
      const list = await invoke<SubmissionSummary[]>("cmd_list_submissions", { assignmentId: id });
      if (currentAssignment.current === id) setSubmissions(list);
    } catch (error) { setError(String(error)); }
  }

  async function createAssignment() {
    if (rubricId === null) return;
    setLoading(true);
    setError(null);
    try {
      const id = await invoke<number>("cmd_create_assignment", { rubricId, text: assignmentText, materialReferences: null });
      const list = await invoke<AssignmentSummary[]>("cmd_list_assignments");
      setAssignments(list);
      selectAssignment(id, list);
    } catch (error) { setError(String(error)); } finally { setLoading(false); }
  }

  async function importFile() {
    if (assignmentId === null) return;
    const id = assignmentId;
    setImporting(true);
    setError(null);
    setNotice("");
    setProgress(null);
    try {
      const selection = await open({ multiple: false, filters: [{ name: "Documents", extensions: ["pdf", "png", "jpg", "jpeg"] }] });
      if (!selection || Array.isArray(selection)) return;
      const token = crypto.randomUUID();
      requestId.current = token;
      const channel = new Channel<ImportProgress>();
      channel.onmessage = (message) => { if (requestId.current === message.request_id) setProgress(message); };
      await invoke<number>("cmd_import_submission", { assignmentId: id, filePath: selection, requestId: token, onProgress: channel });
      await reloadSubmissions(id);
      const list = await invoke<AssignmentSummary[]>("cmd_list_assignments");
      setAssignments(list);
      setNotice("Submission imported and saved.");
    } catch (error) { setError(String(error)); } finally {
      requestId.current = null;
      setImporting(false);
      setProgress(null);
    }
  }

  async function cancelImport() {
    if (!requestId.current) return;
    try {
      await invoke("cmd_cancel_import", { requestId: requestId.current });
      setNotice("Cancellation requested. The current page will finish before stopping.");
    } catch (error) { setError(String(error)); }
  }

  async function exportCsv() {
    if (assignmentId === null) return;
    setError(null);
    try {
      const destination = await save({ defaultPath: "grades.csv", filters: [{ name: "CSV", extensions: ["csv"] }] });
      if (!destination) return;
      const rows = await invoke<number>("cmd_export_csv", { assignmentId, destinationPath: destination });
      setNotice(`Exported ${rows} confirmed grades to ${destination}`);
    } catch (error) { setError(String(error)); }
  }

  const activeRubricId = assignments.find((item) => item.id === assignmentId)?.rubric_id;
  const activeAssignment = assignments.find((item) => item.id === assignmentId);
  const criteria = rubrics.find((item) => item.id === activeRubricId)?.criteria ?? [];
  return (
    <section aria-labelledby="submissions-title">
      <h2 id="submissions-title">Submissions</h2>
      <p className="page-description">Reopen a saved assignment or create a new one. Import PDF, PNG, or JPEG files up to 50 MiB and 50 pages.</p>
      <div className="assignment-layout">
      <div className="saved-work">
      <h3>Continue saved work</h3>
      <label>Saved assignment:
        <select value={assignmentId ?? ""} disabled={loading || importing} onChange={(event) => selectAssignment(Number(event.currentTarget.value) || null)}>
          <option value="">Select an assignment</option>
          {assignments.map((item) => <option key={item.id} value={item.id}>#{item.id} - {item.text.slice(0, 60)} ({item.submission_count} submissions)</option>)}
        </select>
      </label>
      {activeAssignment && <div className="assignment-preview">
        <p><strong>Active assignment: #{assignmentId}</strong></p>
        <p>{activeAssignment.text}</p>
        <span className="supporting-text">{rubrics.find((item) => item.id === activeRubricId)?.title} · {activeAssignment.submission_count} submissions</span>
      </div>}
      {!loading && assignments.length === 0 && <p className="supporting-text">No saved assignments yet. Select a rubric and create your first assignment.</p>}
      {assignmentId !== null && <div className="action-row">
        <button className="button-primary" onClick={importFile} disabled={loading || importing}>Import PDF/image</button>
        <button onClick={exportCsv} disabled={loading || importing}>Export confirmed grades (CSV)</button>
      </div>}
      </div>
      <fieldset disabled={loading || importing}>
        <legend>New assignment</legend>
        <label>Rubric:
          <select value={rubricId ?? ""} onChange={(event) => setRubricId(Number(event.currentTarget.value) || null)}>
            <option value="">Select a rubric</option>
            {rubrics.map((item) => <option key={item.id} value={item.id}>#{item.id} - {item.title}</option>)}
          </select>
        </label>
        <label>Assignment instructions:
          <textarea value={assignmentText} maxLength={65536} onChange={(event) => setAssignmentText(event.currentTarget.value)} />
        </label>
        <button className="button-primary" onClick={createAssignment} disabled={rubricId === null || !assignmentText.trim()}>Create assignment</button>
        {!loading && rubrics.length === 0 && <p className="supporting-text">Create a synthetic rubric in the Rubrics section before adding an assignment.</p>}
      </fieldset>
      </div>
      {importing && <div className="import-progress" role="status" aria-live="polite">
        <p>{progress ? `Page ${progress.page} of ${progress.total}: ${progress.stage}` : "Preparing import..."}</p>
        {progress && progress.total > 0 && <progress aria-label="Import progress" value={progress.page} max={progress.total} />}
        <button onClick={cancelImport} disabled={!progress}>Cancel import</button>
      </div>}
      {loading && <p role="status">Loading saved work...</p>}
      {notice && <p className="notice" role="status">{notice}</p>}
      {error && <pre role="alert">{error}</pre>}
      <div className="section-heading"><h3>Processed submissions ({submissions.length})</h3></div>
      {!loading && submissions.length === 0 && <div className="empty-state">
        <strong>{assignmentId === null ? "Choose an assignment to begin" : "Ready for the first submission"}</strong>
        <p>{assignmentId === null ? "Your saved submissions will appear here when you select an assignment." : "Import a PDF or image, review its redaction, then enter your assessment."}</p>
      </div>}
      <ul className="submission-list">{submissions.map((submission) => <li className="submission-item" key={submission.id}>
        <div className="submission-heading">
          <div><h4>Submission #{submission.id}{submission.student_name ? ` — ${submission.student_name}` : ""}</h4>
          <span className="supporting-text">{submission.method_ocr === "text_native" ? "PDF text extraction" : submission.method_ocr === "ocr_local" ? "Optical character recognition" : submission.method_ocr ?? "Extraction method unavailable"}</span></div>
          <span className={`status-label ${submission.status_pipeline === "grade_confirmed" ? "status-confirmed" : submission.status_pipeline === "redacted" ? "status-reviewed" : ""}`}>
            {submission.status_pipeline === "grade_confirmed" ? "Grade confirmed" : submission.status_pipeline === "redacted" ? "Ready to grade" : "Redaction review needed"}
          </span>
        </div>
        <details><summary>Original extracted text (local only)</summary><pre>{submission.text_ocr}</pre></details>
        <RedactionPanel submission={submission} onConfirmed={() => { void reloadSubmissions(submission.assignment_id); }} />
        <GradingPanel submission={submission} criteria={criteria} onGradeConfirmed={() => { void reloadSubmissions(submission.assignment_id); }} />
      </li>)}</ul>
    </section>
  );
}
