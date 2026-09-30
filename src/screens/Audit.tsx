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

  async function reload() {
    setError(null);
    try {
      const list = await invoke<AuditLog[]>("cmd_list_logs_audit", {
        submissionId: null,
      });
      setLogs(list);
    } catch (e) {
      setError(String(e));
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
      alert(`Exported ${rows} audit entries to ${destination}`);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section>
      <h2>Audit (Milestone 7)</h2>
      <p>Exports include event metadata only. Original text, evidence, feedback, and student identities are excluded.</p>
      <button onClick={reload}>Reload</button>
      <button onClick={exportCsv}>Export CSV</button>
      {error && <pre role="alert" style={{ color: "crimson" }}>{error}</pre>}
      <table style={{ width: "100%", marginTop: "0.5rem", borderCollapse: "collapse" }}>
        <thead>
          <tr>
            <th style={{ textAlign: "left" }}>Date</th>
            <th style={{ textAlign: "left" }}>Submission</th>
            <th style={{ textAlign: "left" }}>Event</th>
            <th style={{ textAlign: "left" }}>Actor</th>
            <th style={{ textAlign: "left" }}>Model</th>
          </tr>
        </thead>
        <tbody>
          {logs.map((log) => (
            <tr key={log.id} style={{ borderTop: "1px solid #ddd" }}>
              <td>{log.created_at}</td>
              <td>{log.submission_id ?? "—"}</td>
              <td>{log.event}</td>
              <td>{log.actor}</td>
              <td>{log.model_version ?? "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
