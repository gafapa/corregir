import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { AuditLog } from "../lib/types";

/**
 * Milestone 7: audit trail required by Article 12 of the AI Act. Shows the
 * complete chain for each submission: OCR, redaction, AI calls and model
 * version, and grade confirmation.
 */
export function Audit() {
  const [logs, setLogs] = useState<AuditLog[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  async function reload() {
    setLoading(true);
    setError(null);
    try {
      const list = await invoke<AuditLog[]>("cmd_list_logs_audit", {
        submissionId: null,
      });
      setLogs(list);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    reload();
  }, []);

  async function exportCsv() {
    setError(null);
    try {
      const destination = await save({ defaultPath: "audit.csv", filters: [{ name: "CSV", extensions: ["csv"] }] });
      if (!destination) return;
      const rows = await invoke<number>("cmd_export_logs_csv", {
        submissionId: null,
        destinationPath: destination,
      });
      setNotice(`Exported ${rows} audit entries to ${destination}`);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section>
      <h2>Audit (Milestone 7)</h2>
      <p className="page-description">Exports include event metadata only. Original text, evidence, feedback, and student identities are excluded.</p>
      <div className="action-row">
      <button onClick={reload} disabled={loading}>Reload</button>
      <button className="button-primary" onClick={exportCsv} disabled={loading}>Export CSV</button>
      </div>
      {notice && <p className="notice" role="status">{notice}</p>}
      {error && <pre role="alert">{error}</pre>}
      {loading && <p className="assessment-status" role="status">Loading audit events…</p>}
      {!loading && logs.length === 0 && !error && <div className="empty-state"><strong>No audit events yet</strong><p>Import and review a submission to start recording its workflow.</p></div>}
      <div className="table-container" role="region" aria-label="Audit events" tabIndex={0}>
      <table>
        <thead>
          <tr>
            <th scope="col">Date</th>
            <th scope="col">Submission</th>
            <th scope="col">Event</th>
            <th scope="col">Actor</th>
            <th scope="col">Model</th>
          </tr>
        </thead>
        <tbody>
          {logs.map((log) => (
            <tr key={log.id}>
              <td>{log.created_at}</td>
              <td>{log.submission_id ?? "—"}</td>
              <td>{log.event}</td>
              <td>{log.actor}</td>
              <td>{log.model_version ?? "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>
    </section>
  );
}
