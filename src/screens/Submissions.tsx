import { useEffect, useMemo, useRef, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { assignmentLabel, type AssignmentSummary, type ClassRoster, type ImportProgress, type RubricWithCriteria, type SubmissionSummary } from "../lib/types";
import { useI18n, type MessageKey } from "../lib/i18n";
import { RedactionPanel } from "./RedactionPanel";
import { DocumentReview } from "./DocumentReview";
import { GradingPanel } from "./GradingPanel";

const STATUS_LABELS: Record<string, MessageKey> = {
  ocr_complete: "status.redactionNeeded",
  redacted: "status.readyToGrade",
  grade_confirmed: "status.confirmed",
};
const STATUS_CLASSES: Record<string, string> = { redacted: "status-reviewed", grade_confirmed: "status-confirmed" };

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

export function submissionTitle(submission: SubmissionSummary, t: ReturnType<typeof useI18n>["t"]) {
  return submission.student_name ?? submission.source_name ?? t("submissions.number", { id: submission.id });
}

type Editor = { mode: "new" } | { mode: "edit"; id: number };
type BatchProgress = { kind: "import" | "evidence"; index: number; total: number; name: string };

export function Submissions() {
  const { t, language, errorText } = useI18n();
  const [rubrics, setRubrics] = useState<RubricWithCriteria[]>([]);
  const [assignments, setAssignments] = useState<AssignmentSummary[]>([]);
  const [classes, setClasses] = useState<ClassRoster[]>([]);
  const [assignmentId, setAssignmentId] = useState<number | null>(null);
  const [submissions, setSubmissions] = useState<SubmissionSummary[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [batch, setBatch] = useState<BatchProgress | null>(null);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [failures, setFailures] = useState<string[]>([]);
  const [notice, setNotice] = useState("");
  const [forceOcr, setForceOcr] = useState(false);
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState("all");
  const [dirtyTexts, setDirtyTexts] = useState<Set<number>>(new Set());
  const [editor, setEditor] = useState<Editor | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const currentAssignment = useRef<number | null>(null);
  const requestId = useRef<string | null>(null);
  const stopBatch = useRef(false);
  const busy = loading || batch !== null;
  const blocked = busy || dirtyTexts.size > 0;

  useEffect(() => {
    window.dispatchEvent(new CustomEvent("corregir-text-draft", { detail: dirtyTexts.size > 0 }));
    return () => { window.dispatchEvent(new CustomEvent("corregir-text-draft", { detail: false })); };
  }, [dirtyTexts]);

  function selectAssignment(id: number | null, list = assignments) {
    const assignment = list.find(item => item.id === id);
    currentAssignment.current = assignment?.id ?? null;
    setAssignmentId(assignment?.id ?? null);
    setSubmissions([]); setSelectedId(null);
    setSearch(""); setStatusFilter("all"); setDirtyTexts(new Set()); setFailures([]);
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
      invoke<ClassRoster[]>("cmd_list_classes").catch(() => []),
    ]).then(([rubricList, assignmentList, classList]) => {
      if (!active) return;
      setRubrics(rubricList);
      setAssignments(assignmentList);
      setClasses(classList ?? []);
      let savedId = 0;
      try { savedId = Number(localStorage.getItem("corregir.activeAssignmentId")); } catch { /* Use the newest assignment. */ }
      const selected = assignmentList.find(item => item.id === savedId) ?? assignmentList[0];
      if (selected) selectAssignment(selected.id, assignmentList);
      else setEditor({ mode: "new" });
    }).catch(cause => { if (active) setError(errorText(cause)); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    if (assignmentId === null) return;
    let active = true;
    setLoading(true);
    invoke<SubmissionSummary[]>("cmd_list_submissions", { assignmentId }).then(list => {
      if (!active) return;
      setSubmissions(list);
      setSelectedId(previous => previous ?? list.find(item => item.status_pipeline !== "grade_confirmed")?.id ?? list[0]?.id ?? null);
    }).catch(cause => { if (active) setError(errorText(cause)); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [assignmentId]);

  async function reloadSubmissions(id: number) {
    try {
      const [list, assignmentList] = await Promise.all([
        invoke<SubmissionSummary[]>("cmd_list_submissions", { assignmentId: id }),
        invoke<AssignmentSummary[]>("cmd_list_assignments"),
      ]);
      if (currentAssignment.current !== id) return;
      setSubmissions(list);
      setAssignments(assignmentList);
      setSelectedId(previous => list.some(item => item.id === previous) ? previous : list[0]?.id ?? null);
    } catch (cause) { setError(errorText(cause)); }
  }

  async function importFiles() {
    if (assignmentId === null) return;
    const id = assignmentId;
    setError(null); setNotice(""); setFailures([]); setProgress(null);
    let files: string[];
    try {
      const selection = await open({ multiple: true, filters: [{ name: t("submissions.documents"), extensions: ["pdf", "docx", "png", "jpg", "jpeg"] }] });
      files = selection === null ? [] : Array.isArray(selection) ? selection : [selection];
    } catch (cause) { setError(errorText(cause)); return; }
    if (files.length === 0) return;
    stopBatch.current = false;
    const problems: string[] = [];
    let imported = 0;
    let firstId: number | null = null;
    try {
      for (const [index, file] of files.entries()) {
        if (stopBatch.current) break;
        setBatch({ kind: "import", index: index + 1, total: files.length, name: fileName(file) });
        setProgress(null);
        const token = crypto.randomUUID();
        requestId.current = token;
        const channel = new Channel<ImportProgress>();
        channel.onmessage = message => { if (requestId.current === message.request_id) setProgress(message); };
        try {
          const submissionId = await invoke<number>("cmd_import_submission", { assignmentId: id, filePath: file, requestId: token, onProgress: channel, forceOcr });
          firstId ??= submissionId;
          imported += 1;
        } catch (cause) {
          if (stopBatch.current) break;
          problems.push(`${fileName(file)}: ${errorText(cause)}`);
        }
      }
    } finally {
      requestId.current = null;
      setBatch(null); setProgress(null);
    }
    await reloadSubmissions(id);
    if (firstId !== null) setSelectedId(firstId);
    setFailures(problems);
    setNotice(stopBatch.current ? t("import.cancelled", { imported, total: files.length }) : t("import.done", { imported, total: files.length }));
  }

  async function cancelBatch() {
    stopBatch.current = true;
    if (!requestId.current) return;
    try {
      await invoke(batch?.kind === "evidence" ? "cmd_cancel_inference" : "cmd_cancel_import", { requestId: requestId.current });
    } catch (cause) { setError(errorText(cause)); }
  }

  async function findAllEvidence() {
    if (assignmentId === null) return;
    const id = assignmentId;
    const targets = submissions.filter(item => item.status_pipeline === "redacted" && !item.has_evidence);
    setError(null); setNotice(""); setFailures([]);
    stopBatch.current = false;
    const problems: string[] = [];
    let done = 0;
    try {
      for (const [index, submission] of targets.entries()) {
        if (stopBatch.current) break;
        setBatch({ kind: "evidence", index: index + 1, total: targets.length, name: submissionTitle(submission, t) });
        const token = crypto.randomUUID();
        requestId.current = token;
        try {
          await invoke("cmd_request_evidence", { submissionId: submission.id, requestId: token, onProgress: new Channel() });
          done += 1;
        } catch (cause) {
          if (stopBatch.current) break;
          problems.push(`${submissionTitle(submission, t)}: ${errorText(cause)}`);
        }
      }
    } finally {
      requestId.current = null;
      setBatch(null);
    }
    await reloadSubmissions(id);
    setRefreshKey(key => key + 1);
    setFailures(problems);
    setNotice(t("evidence.batchDone", { done, total: targets.length }));
  }

  async function exportCsv() {
    if (assignmentId === null) return;
    setError(null); setNotice("");
    try {
      const destination = await save({ defaultPath: t("export.csvName"), filters: [{ name: "CSV", extensions: ["csv"] }] });
      if (!destination) return;
      const rows = await invoke<number>("cmd_export_csv", { assignmentId, destinationPath: destination, language });
      setNotice(t("export.csvDone", { rows, path: destination }));
    } catch (cause) { setError(errorText(cause)); }
  }

  async function exportAllPdfs() {
    if (assignmentId === null) return;
    setError(null); setNotice("");
    try {
      const directory = await open({ directory: true, multiple: false, title: t("export.chooseFolder") });
      if (typeof directory !== "string") return;
      const files = await invoke<number>("cmd_export_pdf_batch", { assignmentId, directoryPath: directory, language });
      setNotice(t("export.pdfBatchDone", { files, path: directory }));
    } catch (cause) { setError(errorText(cause)); }
  }

  async function deleteAssignment() {
    if (!activeAssignment) return;
    const confirmed = await ask(t("assignment.deleteConfirm", { name: assignmentLabel(activeAssignment), count: activeAssignment.submission_count }), { title: t("assignment.delete"), kind: "warning", okLabel: t("common.delete"), cancelLabel: t("common.cancel") });
    if (!confirmed) return;
    setLoading(true); setError(null);
    try {
      await invoke("cmd_delete_assignment", { assignmentId: activeAssignment.id });
      const list = await invoke<AssignmentSummary[]>("cmd_list_assignments");
      setAssignments(list);
      selectAssignment(list[0]?.id ?? null, list);
      setNotice(t("assignment.deleted"));
    } catch (cause) { setError(errorText(cause)); } finally { setLoading(false); }
  }

  async function deleteSubmission(submission: SubmissionSummary) {
    const confirmed = await ask(t("submission.deleteConfirm", { name: submissionTitle(submission, t) }), { title: t("submission.delete"), kind: "warning", okLabel: t("common.delete"), cancelLabel: t("common.cancel") });
    if (!confirmed) return;
    setError(null);
    try {
      await invoke("cmd_delete_submission", { submissionId: submission.id });
      setSelectedId(null);
      await reloadSubmissions(submission.assignment_id);
      setNotice(t("submission.deleted"));
    } catch (cause) { setError(errorText(cause)); }
  }

  const activeAssignment = assignments.find(item => item.id === assignmentId);
  const activeRubric = rubrics.find(item => item.id === activeAssignment?.rubric_id);
  const activeClass = classes.find(item => item.id === activeAssignment?.class_id);
  const criteria = activeRubric?.criteria ?? [];
  const filteredSubmissions = useMemo(() => submissions.filter(item => {
    const matchesStatus = statusFilter === "all" || item.status_pipeline === statusFilter;
    const needle = search.trim().toLocaleLowerCase();
    return matchesStatus && (!needle || String(item.id).includes(needle) || (item.student_name ?? "").toLocaleLowerCase().includes(needle) || (item.source_name ?? "").toLocaleLowerCase().includes(needle) || (item.text_ocr ?? "").toLocaleLowerCase().includes(needle));
  }), [submissions, search, statusFilter]);
  const selected = submissions.find(item => item.id === selectedId) ?? null;
  const linkedStudents = submissions.filter(item => item.student_id != null && item.id !== selectedId).map(item => item.student_id as number);
  const pendingEvidence = submissions.filter(item => item.status_pipeline === "redacted" && !item.has_evidence).length;
  const confirmedCount = submissions.filter(item => item.status_pipeline === "grade_confirmed").length;

  function nextPending() {
    const pending = filteredSubmissions.filter(item => item.status_pipeline !== "grade_confirmed" && item.id !== selectedId);
    const next = pending.find(item => item.id > (selectedId ?? 0)) ?? pending[0];
    if (!next) return;
    setSelectedId(next.id);
    window.setTimeout(() => document.getElementById("submission-detail")?.focus(), 0);
  }

  return (
    <section aria-labelledby="submissions-title">
      <h2 id="submissions-title">{t("submissions.title")}</h2>
      <p className="page-description">{t("submissions.description")}</p>

      <div className="assignment-bar">
        <label className="assignment-picker">{t("assignment.saved")}
          <select value={assignmentId ?? ""} disabled={blocked} onChange={event => { setEditor(null); selectAssignment(Number(event.currentTarget.value) || null); }}>
            <option value="">{t("assignment.select")}</option>
            {assignments.map(item => <option key={item.id} value={item.id}>{assignmentLabel(item)} ({t("assignment.count", { count: item.submission_count })})</option>)}
          </select>
        </label>
        <button className={editor?.mode === "new" ? "" : "button-primary"} disabled={blocked} onClick={() => setEditor(editor?.mode === "new" ? null : { mode: "new" })}>{t("assignment.new")}</button>
      </div>

      {editor && <AssignmentEditor
        key={editor.mode === "edit" ? editor.id : "new"}
        assignment={editor.mode === "edit" ? activeAssignment : undefined}
        rubrics={rubrics}
        classes={classes}
        disabled={blocked}
        onCancel={() => setEditor(null)}
        onSaved={async (id) => {
          const list = await invoke<AssignmentSummary[]>("cmd_list_assignments");
          setAssignments(list);
          setEditor(null);
          if (id !== assignmentId) selectAssignment(id, list);
          setNotice(t("assignment.saved.notice"));
        }}
      />}

      {activeAssignment && !editor && <div className="assignment-summary">
        <div className="assignment-heading">
          <div>
            <h3>{assignmentLabel(activeAssignment)}</h3>
            <p className="supporting-text">{activeRubric ? `${activeRubric.title} · v${activeRubric.version}` : t("assignment.rubricMissing")} · {activeClass ? t("assignment.classLabel", { name: activeClass.name, count: activeClass.students.length }) : t("assignment.noClass")}</p>
          </div>
          <div className="action-row compact">
            <button className="button-quiet" disabled={blocked} onClick={() => setEditor({ mode: "edit", id: activeAssignment.id })}>{t("assignment.edit")}</button>
            <button className="button-quiet button-danger" disabled={blocked} onClick={() => void deleteAssignment()}>{t("assignment.delete")}</button>
          </div>
        </div>
        <details className="assignment-instructions"><summary>{t("assignment.instructions")}</summary><p>{activeAssignment.text}</p></details>
        <ul className="progress-counts" aria-label={t("assignment.progress")}>
          <li><strong>{activeAssignment.redaction_pending ?? 0}</strong> {t("status.redactionNeeded")}</li>
          <li><strong>{activeAssignment.ready_to_grade ?? 0}</strong> {t("status.readyToGrade")}</li>
          <li><strong>{activeAssignment.confirmed ?? 0}</strong> {t("status.confirmed")}</li>
        </ul>
        <div className="action-row">
          <button className="button-primary" onClick={() => void importFiles()} disabled={blocked}>{t("import.button")}</button>
          <button onClick={() => void findAllEvidence()} disabled={blocked || pendingEvidence === 0}>{t("evidence.batchButton", { count: pendingEvidence })}</button>
          <button onClick={() => void exportCsv()} disabled={blocked || confirmedCount === 0}>{t("export.csvButton")}</button>
          <button onClick={() => void exportAllPdfs()} disabled={blocked || confirmedCount === 0}>{t("export.pdfBatchButton")}</button>
        </div>
        <label className="inline-check"><input type="checkbox" checked={forceOcr} disabled={blocked} onChange={event => setForceOcr(event.currentTarget.checked)} />{t("import.forceOcr")}</label>
        <p className="supporting-text">{t("import.help")}</p>
      </div>}

      {batch && <div className="import-progress" role="status" aria-live="polite">
        <p>{t(batch.kind === "import" ? "import.progressFile" : "evidence.progress", { index: batch.index, total: batch.total, name: batch.name })}</p>
        {batch.kind === "import" && <p className="supporting-text">{progress && progress.total > 0 ? t("import.progressPage", { page: progress.page, total: progress.total }) : t("import.preparing")}</p>}
        <progress aria-label={t("import.progressLabel")} value={batch.index - 1} max={batch.total} />
        <button onClick={() => void cancelBatch()}>{t("common.stop")}</button>
      </div>}
      {loading && <p role="status">{t("submissions.loading")}</p>}
      {notice && <p className="notice" role="status">{notice}</p>}
      {failures.length > 0 && <div role="alert"><p>{t("batch.failures")}</p><ul>{failures.map(item => <li key={item}>{item}</li>)}</ul></div>}
      {error && <pre role="alert">{error}</pre>}

      {assignmentId !== null && !loading && submissions.length === 0 && <div className="empty-state">
        <strong>{t("submissions.emptyTitle")}</strong>
        <p>{t("submissions.emptyText")}</p>
      </div>}
      {assignmentId === null && !loading && !editor && <div className="empty-state"><strong>{t("submissions.chooseTitle")}</strong><p>{t("submissions.chooseText")}</p></div>}

      {submissions.length > 0 && <div className="workbench">
        <aside className="submission-sidebar" aria-label={t("submissions.listLabel")}>
          <label>{t("submissions.search")}<input type="search" disabled={dirtyTexts.size > 0} value={search} onChange={event => setSearch(event.currentTarget.value)} placeholder={t("submissions.searchPlaceholder")} /></label>
          <label>{t("submissions.statusFilter")}<select disabled={dirtyTexts.size > 0} value={statusFilter} onChange={event => setStatusFilter(event.currentTarget.value)}>
            <option value="all">{t("status.all")}</option>
            <option value="ocr_complete">{t("status.redactionNeeded")}</option>
            <option value="redacted">{t("status.readyToGrade")}</option>
            <option value="grade_confirmed">{t("status.confirmed")}</option>
          </select></label>
          <p className="supporting-text" role="status">{t("submissions.showing", { shown: filteredSubmissions.length, total: submissions.length })}</p>
          {filteredSubmissions.length === 0 && <p>{t("submissions.noMatch")}</p>}
          <ul className="submission-index">
            {filteredSubmissions.map(item => <li key={item.id}>
              <button className={item.id === selectedId ? "selected" : ""} aria-current={item.id === selectedId ? "true" : undefined} disabled={dirtyTexts.size > 0 && item.id !== selectedId} onClick={() => setSelectedId(item.id)}>
                <span className="submission-index-name">{submissionTitle(item, t)}</span>
                <span className={`status-dot ${STATUS_CLASSES[item.status_pipeline] ?? ""}`}>{t(STATUS_LABELS[item.status_pipeline] ?? "status.redactionNeeded")}</span>
              </button>
            </li>)}
          </ul>
        </aside>

        {selected ? <article className="submission-item" id="submission-detail" tabIndex={-1} aria-labelledby={`submission-title-${selected.id}`}>
          <div className="submission-heading">
            <div>
              <h4 id={`submission-title-${selected.id}`}>{submissionTitle(selected, t)}</h4>
              <span className="supporting-text">
                {t("submissions.number", { id: selected.id })}
                {selected.source_name && selected.student_name ? ` · ${selected.source_name}` : ""}
                {" · "}{selected.method_ocr === "text_native" ? t("method.native") : selected.method_ocr === "ocr_local" ? t("method.ocr") : t("method.unknown")}
                {selected.identities_purged ? ` · ${t("submissions.pseudonymized")}` : ""}
              </span>
            </div>
            <span className={`status-label ${STATUS_CLASSES[selected.status_pipeline] ?? ""}`}>{t(STATUS_LABELS[selected.status_pipeline] ?? "status.redactionNeeded")}</span>
          </div>
          <div className="action-row compact">
            <button className="button-quiet" onClick={nextPending} disabled={dirtyTexts.size > 0 || !filteredSubmissions.some(item => item.status_pipeline !== "grade_confirmed" && item.id !== selected.id)}>{t("submissions.next")}</button>
            <button className="button-quiet button-danger" onClick={() => void deleteSubmission(selected)} disabled={blocked}>{t("submission.delete")}</button>
          </div>
          {!selected.identities_purged && <DocumentReview key={`review-${selected.id}`} submission={selected} onSaved={() => reloadSubmissions(selected.assignment_id)} onDraftChange={dirty => setDirtyTexts(previous => { const next = new Set(previous); if (dirty) next.add(selected.id); else next.delete(selected.id); return next; })} />}
          <RedactionPanel key={`redaction-${selected.id}`} blocked={dirtyTexts.has(selected.id)} submission={selected} roster={activeClass?.students ?? []} linkedStudentIds={linkedStudents} onConfirmed={() => { void reloadSubmissions(selected.assignment_id); }} />
          <GradingPanel key={`grading-${selected.id}-${refreshKey}`} submission={selected} criteria={criteria} onChanged={() => { void reloadSubmissions(selected.assignment_id); }} onGradeConfirmed={() => { void reloadSubmissions(selected.assignment_id); }} />
        </article> : <div className="empty-state"><strong>{t("submissions.selectTitle")}</strong><p>{t("submissions.selectText")}</p></div>}
      </div>}
    </section>
  );
}

function AssignmentEditor({ assignment, rubrics, classes, disabled, onCancel, onSaved }: {
  assignment?: AssignmentSummary;
  rubrics: RubricWithCriteria[];
  classes: ClassRoster[];
  disabled: boolean;
  onCancel: () => void;
  onSaved: (id: number) => Promise<void>;
}) {
  const { t, errorText } = useI18n();
  const [title, setTitle] = useState(assignment?.title ?? "");
  const [rubricId, setRubricId] = useState<number | null>(assignment?.rubric_id ?? null);
  const [classId, setClassId] = useState<number | null>(assignment?.class_id ?? null);
  const [text, setText] = useState(assignment?.text ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true); setError(null);
    try {
      if (assignment) {
        await invoke("cmd_update_assignment", { assignmentId: assignment.id, title: title.trim() || null, classId });
        await onSaved(assignment.id);
      } else {
        const id = await invoke<number>("cmd_create_assignment", { rubricId, text, materialReferences: null, title: title.trim() || null, classId });
        await onSaved(id);
      }
    } catch (cause) { setError(errorText(cause)); } finally { setBusy(false); }
  }
  return <form onSubmit={event => void submit(event)} className="assignment-editor">
    <fieldset disabled={disabled || busy}>
      <legend>{assignment ? t("assignment.editTitle") : t("assignment.newTitle")}</legend>
      <label>{t("assignment.titleField")}<input maxLength={200} value={title} onChange={event => setTitle(event.currentTarget.value)} placeholder={t("assignment.titlePlaceholder")} /></label>
      <div className="form-grid">
        <label>{t("assignment.rubric")}
          <select value={rubricId ?? ""} required disabled={!!assignment} onChange={event => setRubricId(Number(event.currentTarget.value) || null)}>
            <option value="">{t("assignment.selectRubric")}</option>
            {rubrics.map(item => <option key={item.id} value={item.id}>{item.title} · v{item.version}</option>)}
          </select>
        </label>
        <label>{t("assignment.class")}
          <select value={classId ?? ""} onChange={event => setClassId(Number(event.currentTarget.value) || null)}>
            <option value="">{t("assignment.noClass")}</option>
            {classes.map(item => <option key={item.id} value={item.id}>{item.name} ({t("classes.count", { count: item.students.length })})</option>)}
          </select>
        </label>
      </div>
      {!assignment && <label>{t("assignment.instructionsField")}
        <textarea required value={text} maxLength={65536} onChange={event => setText(event.currentTarget.value)} placeholder={t("assignment.instructionsPlaceholder")} />
      </label>}
      {assignment && <p className="supporting-text">{t("assignment.rubricLocked")}</p>}
      {rubrics.length === 0 && <p className="supporting-text">{t("assignment.noRubrics")}</p>}
      {classes.length === 0 && <p className="supporting-text">{t("assignment.noClasses")}</p>}
      <div className="action-row">
        <button className="button-primary" type="submit" disabled={!assignment && (rubricId === null || !text.trim())}>{assignment ? t("common.save") : t("assignment.create")}</button>
        <button type="button" onClick={onCancel}>{t("common.cancel")}</button>
      </div>
      {error && <pre role="alert">{error}</pre>}
    </fieldset>
  </form>;
}
