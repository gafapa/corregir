import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask, save } from "@tauri-apps/plugin-dialog";
import { assignmentLabel, type AssignmentSummary, type KnownStudent, type Preferences, type RetentionDue } from "../lib/types";
import { useI18n } from "../lib/i18n";

/** Retention, pseudonymization and per-student access and erasure requests. */
export function Privacy() {
  const { t, formatDate, errorText } = useI18n();
  const [preferences, setPreferences] = useState<Preferences | null>(null);
  const [retentionDays, setRetentionDays] = useState("");
  const [due, setDue] = useState<RetentionDue[]>([]);
  const [assignments, setAssignments] = useState<AssignmentSummary[]>([]);
  const [students, setStudents] = useState<KnownStudent[]>([]);
  const [student, setStudent] = useState("");
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("");

  async function reload() {
    const [loadedPreferences, loadedDue, loadedAssignments, loadedStudents] = await Promise.all([
      invoke<Preferences>("cmd_load_preferences"),
      invoke<RetentionDue[]>("cmd_retention_status"),
      invoke<AssignmentSummary[]>("cmd_list_assignments"),
      invoke<KnownStudent[]>("cmd_list_known_students"),
    ]);
    setPreferences(loadedPreferences);
    setRetentionDays(loadedPreferences.retention_days ? String(loadedPreferences.retention_days) : "");
    setDue(loadedDue);
    setAssignments(loadedAssignments);
    setStudents(loadedStudents);
    setStudent(previous => loadedStudents.some(item => item.name === previous) ? previous : "");
  }
  useEffect(() => {
    let active = true;
    reload().catch(cause => { if (active) setError(errorText(cause)); }).finally(() => { if (active) setBusy(false); });
    return () => { active = false; };
  }, []);

  async function run(action: () => Promise<string>) {
    setBusy(true); setError(null); setNotice("");
    try { const message = await action(); await reload(); setNotice(message); }
    catch (cause) { setError(errorText(cause)); } finally { setBusy(false); }
  }
  function saveRetention() {
    if (!preferences) return;
    const days = retentionDays.trim() ? Number(retentionDays) : null;
    void run(async () => {
      await invoke("cmd_save_preferences", { preferences: { ...preferences, retention_days: days } });
      return days ? t("privacy.retentionSaved", { days }) : t("privacy.retentionDisabled");
    });
  }
  async function purge(assignment: { id: number; label: string }) {
    const confirmed = await ask(t("privacy.purgeConfirm", { name: assignment.label }), { title: t("privacy.purge"), kind: "warning", okLabel: t("privacy.purge"), cancelLabel: t("common.cancel") });
    if (!confirmed) return;
    void run(async () => {
      const summary = await invoke<{ purged: number; pending: number }>("cmd_purge_assignment_identities", { assignmentId: assignment.id });
      return t("privacy.purged", { purged: summary.purged, pending: summary.pending });
    });
  }
  async function exportStudent() {
    if (!student) return;
    const destination = await save({ defaultPath: `${student.replace(/[\\/:*?"<>|]/g, "_")}.json`, filters: [{ name: "JSON", extensions: ["json"] }] }).catch((cause: unknown) => { setError(errorText(cause)); return null; });
    if (!destination) return;
    void run(async () => {
      const count = await invoke<number>("cmd_export_student_data", { studentName: student, destinationPath: destination });
      return t("privacy.exported", { count, path: destination });
    });
  }
  async function eraseStudent() {
    if (!student) return;
    const confirmed = await ask(t("privacy.eraseConfirm", { name: student }), { title: t("privacy.erase"), kind: "warning", okLabel: t("privacy.erase"), cancelLabel: t("common.cancel") });
    if (!confirmed) return;
    void run(async () => {
      const summary = await invoke<{ submissions: number; roster_entries: number; mentions: number }>("cmd_erase_student", { studentName: student });
      return t("privacy.erased", { submissions: summary.submissions, roster: summary.roster_entries, mentions: summary.mentions });
    });
  }
  const identified = assignments.filter(item => (item.identified ?? 0) > 0);
  const selected = students.find(item => item.name === student);
  return <section aria-labelledby="privacy-title">
    <h2 id="privacy-title">{t("privacy.title")}</h2>
    <p className="page-description">{t("privacy.description")}</p>
    {busy && <p role="status">{t("common.loading")}</p>}
    {notice && <p className="notice" role="status">{notice}</p>}
    {error && <pre role="alert">{error}</pre>}

    <fieldset disabled={busy}>
      <legend>{t("privacy.retention")}</legend>
      <p className="supporting-text">{t("privacy.retentionHelp")}</p>
      <div className="inline-form">
        <label>{t("privacy.retentionDays")}<input type="number" min={1} max={3650} step={1} value={retentionDays} onChange={event => setRetentionDays(event.currentTarget.value)} placeholder={t("privacy.retentionNone")} /></label>
        <button onClick={saveRetention} disabled={retentionDays.trim() !== "" && !(Number(retentionDays) >= 1 && Number(retentionDays) <= 3650)}>{t("common.save")}</button>
      </div>
      {due.length > 0 && <div className="banner-list" role="status">
        <p className="warning-text">{t("privacy.dueTitle", { count: due.length })}</p>
        <ul>{due.map(item => <li key={item.assignment_id}>
          {assignmentLabel({ id: item.assignment_id, title: item.title, text: item.text })} · {t("privacy.dueDetail", { count: item.identified, date: item.last_confirmed_at ? formatDate(item.last_confirmed_at) : "—" })}
          {" "}<button className="button-quiet" onClick={() => void purge({ id: item.assignment_id, label: assignmentLabel({ id: item.assignment_id, title: item.title, text: item.text }) })}>{t("privacy.purge")}</button>
        </li>)}</ul>
      </div>}
    </fieldset>

    <fieldset disabled={busy}>
      <legend>{t("privacy.purgeTitle")}</legend>
      <p className="supporting-text">{t("privacy.purgeHelp")}</p>
      {identified.length === 0 ? <p className="supporting-text">{t("privacy.nothingToPurge")}</p> : <ul className="plain-list">{identified.map(item => <li key={item.id}>
        <span>{assignmentLabel(item)} · {t("privacy.identifiedCount", { count: item.identified ?? 0 })}</span>{" "}
        <button className="button-quiet" onClick={() => void purge({ id: item.id, label: assignmentLabel(item) })}>{t("privacy.purge")}</button>
      </li>)}</ul>}
    </fieldset>

    <fieldset disabled={busy}>
      <legend>{t("privacy.requests")}</legend>
      <p className="supporting-text">{t("privacy.requestsHelp")}</p>
      <label>{t("privacy.student")}
        <select value={student} onChange={event => setStudent(event.currentTarget.value)}>
          <option value="">{t("privacy.chooseStudent")}</option>
          {students.map(item => <option key={item.name} value={item.name}>{item.name}</option>)}
        </select>
      </label>
      {selected && <p className="supporting-text">{t("privacy.studentRecords", { submissions: selected.submissions, roster: selected.roster_entries })}</p>}
      <div className="action-row">
        <button onClick={() => void exportStudent()} disabled={!student}>{t("privacy.export")}</button>
        <button className="button-danger" onClick={() => void eraseStudent()} disabled={!student}>{t("privacy.erase")}</button>
      </div>
      <p className="supporting-text">{t("privacy.backupNote")}</p>
    </fieldset>
  </section>;
}
