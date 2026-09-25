import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { LogAuditoria } from "../lib/types";

/**
 * Hito 7: registro de trazabilidad exigido por el Art. 12 del Reglamento
 * de IA. Muestra la cadena completa por entrega: OCR, anonimización,
 * llamadas a la IA (con versión de modelo) y confirmación de nota.
 */
export function Auditoria() {
  const [logs, setLogs] = useState<LogAuditoria[]>([]);
  const [error, setError] = useState<string | null>(null);

  async function recargar() {
    setError(null);
    try {
      const lista = await invoke<LogAuditoria[]>("cmd_listar_logs_auditoria", {
        entregaId: null,
      });
      setLogs(lista);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    recargar();
  }, []);

  async function exportarCsv() {
    const destino = await save({
      defaultPath: "auditoria.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!destino) return;
    setError(null);
    try {
      const filas = await invoke<number>("cmd_exportar_logs_csv", {
        entregaId: null,
        rutaDestino: destino,
      });
      alert(`Exportadas ${filas} entradas del registro a ${destino}`);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section>
      <h2>Auditoría (Hito 7)</h2>
      <button onClick={recargar}>Recargar</button>
      <button onClick={exportarCsv}>Exportar a CSV</button>
      {error && <pre style={{ color: "crimson" }}>{error}</pre>}
      <table style={{ width: "100%", marginTop: "0.5rem", borderCollapse: "collapse" }}>
        <thead>
          <tr>
            <th style={{ textAlign: "left" }}>Fecha</th>
            <th style={{ textAlign: "left" }}>Entrega</th>
            <th style={{ textAlign: "left" }}>Evento</th>
            <th style={{ textAlign: "left" }}>Actor</th>
            <th style={{ textAlign: "left" }}>Modelo</th>
          </tr>
        </thead>
        <tbody>
          {logs.map((log) => (
            <tr key={log.id} style={{ borderTop: "1px solid #ddd" }}>
              <td>{log.creado_en}</td>
              <td>{log.entrega_id ?? "—"}</td>
              <td>{log.evento}</td>
              <td>{log.actor}</td>
              <td>{log.version_modelo ?? "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
