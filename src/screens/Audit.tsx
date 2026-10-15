import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { AuditLog } from "../lib/types";
import { useI18n, type MessageKey } from "../lib/i18n";

const EVENTS: Record<string, MessageKey> = {
  ocr_complete: "audit.event.ocr_complete",
  extracted_text_corrected: "audit.event.extracted_text_corrected",
  redaction_confirmed: "audit.event.redaction_confirmed",
  ai_evidence_request: "audit.event.ai_evidence_request",
  assessment_saved: "audit.event.assessment_saved",
  ai_feedback_request: "audit.event.ai_feedback_request",
  feedback_saved: "audit.event.feedback_saved",
  grade_confirmed: "audit.event.grade_confirmed",
  grade_reopened: "audit.event.grade_reopened",
  export_csv: "audit.event.export_csv",
  export_pdf_feedback: "audit.event.export_pdf_feedback",
  submission_deleted: "audit.event.submission_deleted",
  assignment_deleted: "audit.event.assignment_deleted",
  rubric_deleted: "audit.event.rubric_deleted",
  class_saved: "audit.event.class_saved",
  class_deleted: "audit.event.class_deleted",
  identities_purged: "audit.event.identities_purged",
  student_data_exported: "audit.event.student_data_exported",
  student_erased: "audit.event.student_erased",
};
const ACTORS: Record<string, MessageKey> = { teacher: "audit.actor.teacher", ai: "audit.actor.ai", system: "audit.actor.system" };

/**
 * Audit trail required by Article 12 of the AI Act. Shows the complete chain for
 * each submission: OCR, redaction, AI calls and model version, and grade confirmation.
 */
export function Audit() {
  const { t, formatDateTime, errorText } = useI18n();
  const [logs, setLogs] = useState<AuditLog[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  async function reload() {
    setLoading(true);
    setError(null);
    try {
      const list = await invoke<AuditLog[]>("cmd_list_logs_audit", { submissionId: null });
      setLogs(list.slice().reverse());
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void reload();
  }, []);

  async function exportCsv() {
    setError(null);
    try {
      const destination = await save({ defaultPath: t("audit.fileName"), filters: [{ name: "CSV", extensions: ["csv"] }] });
      if (!destination) return;
      const rows = await invoke<number>("cmd_export_logs_csv", { submissionId: null, destinationPath: destination });
      setNotice(t("audit.exported", { rows, path: destination }));
    } catch (cause) {
      setError(errorText(cause));
    }
  }

  return (
    <section aria-labelledby="audit-title">
      <h2 id="audit-title">{t("audit.title")}</h2>
      <p className="page-description">{t("audit.description")}</p>
      <div className="action-row">
        <button onClick={() => void reload()} disabled={loading}>{t("audit.reload")}</button>
        <button className="button-primary" onClick={() => void exportCsv()} disabled={loading}>{t("audit.export")}</button>
      </div>
      {notice && <p className="notice" role="status">{notice}</p>}
      {error && <pre role="alert">{error}</pre>}
      {loading && <p className="assessment-status" role="status">{t("audit.loading")}</p>}
      {!loading && logs.length === 0 && !error && <div className="empty-state"><strong>{t("audit.emptyTitle")}</strong><p>{t("audit.emptyText")}</p></div>}
      <div className="table-container" role="region" aria-label={t("audit.tableLabel")} tabIndex={0}>
      <table>
        <thead>
          <tr>
            <th scope="col">{t("audit.date")}</th>
            <th scope="col">{t("audit.submission")}</th>
            <th scope="col">{t("audit.event")}</th>
            <th scope="col">{t("audit.actor")}</th>
            <th scope="col">{t("audit.model")}</th>
          </tr>
        </thead>
        <tbody>
          {logs.map(log => (
            <tr key={log.id}>
              <td>{formatDateTime(log.created_at)}</td>
              <td>{log.submission_id ?? "—"}</td>
              <td>{EVENTS[log.event] ? t(EVENTS[log.event]) : log.event}</td>
              <td>{ACTORS[log.actor] ? t(ACTORS[log.actor]) : log.actor}</td>
              <td>{log.model_version ?? "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>
    </section>
  );
}
