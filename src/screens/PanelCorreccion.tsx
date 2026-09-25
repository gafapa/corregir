import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { CriterioConId, EntregaResumen, EvidenciaCriterio, FeedbackYConsistencia } from "../lib/types";

/**
 * Hito 5, modo "Asistente de corrección": dos llamadas a la IA separadas
 * por el momento en que el profesor introduce su propia evaluación.
 * La IA nunca propone nota — solo evidencia (antes) y feedback/
 * inconsistencias (después). Solo disponible una vez la entrega está
 * anonimizada (backend lo exige a nivel de tipos, ver
 * pipeline::anonimizacion::TextoAnonimizadoConfirmado).
 */
export function PanelCorreccion({
  entrega,
  criterios,
  onNotaConfirmada,
}: {
  entrega: EntregaResumen;
  criterios: CriterioConId[];
  onNotaConfirmada?: () => void;
}) {
  const [evidencias, setEvidencias] = useState<EvidenciaCriterio[]>([]);
  const [puntuaciones, setPuntuaciones] = useState<Record<string, string>>({});
  const [feedback, setFeedback] = useState<FeedbackYConsistencia | null>(null);
  const [notaConfirmada, setNotaConfirmada] = useState<number | null>(null);
  const [cargando, setCargando] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function exportarPdf() {
    const destino = await save({
      defaultPath: `feedback-entrega-${entrega.id}.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!destino) return;
    setError(null);
    try {
      await invoke("cmd_exportar_pdf_feedback", { entregaId: entrega.id, rutaDestino: destino });
      alert(`Hoja de feedback guardada en ${destino}`);
    } catch (e) {
      setError(String(e));
    }
  }

  if (entrega.estado_pipeline === "nota_confirmada") {
    return (
      <div>
        <p>
          ✅ Nota confirmada para la entrega #{entrega.id}
          {entrega.alumno_nombre ? ` (${entrega.alumno_nombre})` : ""}.
        </p>
        <button onClick={exportarPdf}>Exportar hoja de feedback (PDF)</button>
        {error && <pre style={{ color: "crimson" }}>{error}</pre>}
      </div>
    );
  }
  if (entrega.estado_pipeline !== "anonimizada") {
    return null;
  }

  async function invocarEvidencias() {
    setCargando("evidencias");
    setError(null);
    try {
      const resultado = await invoke<EvidenciaCriterio[]>("cmd_invocar_evidencias", {
        entregaId: entrega.id,
      });
      setEvidencias(resultado);
    } catch (e) {
      setError(String(e));
    } finally {
      setCargando(null);
    }
  }

  async function guardarNotaTentativa() {
    setCargando("nota");
    setError(null);
    try {
      const evaluaciones = criterios
        .filter((c) => puntuaciones[c.codigo] !== undefined && puntuaciones[c.codigo] !== "")
        .map((c) => ({
          // Los nombres de campo dentro de este objeto van tal cual al struct
          // Rust `EvaluacionCriterio` (serde no aplica la conversión
          // snake_case->camelCase que sí aplica Tauri a los argumentos de
          // nivel superior del comando) — deben coincidir exactamente.
          criterio_id: c.codigo,
          puntuacion: Number(puntuaciones[c.codigo]),
          comentario_docente: null,
        }));
      await invoke("cmd_guardar_nota_tentativa", { entregaId: entrega.id, evaluaciones });
    } catch (e) {
      setError(String(e));
    } finally {
      setCargando(null);
    }
  }

  async function confirmarNota() {
    setCargando("confirmar");
    setError(null);
    try {
      const total = await invoke<number>("cmd_confirmar_nota", { entregaId: entrega.id });
      setNotaConfirmada(total);
      onNotaConfirmada?.();
    } catch (e) {
      setError(String(e));
    } finally {
      setCargando(null);
    }
  }

  async function invocarFeedback() {
    setCargando("feedback");
    setError(null);
    try {
      const resultado = await invoke<FeedbackYConsistencia>("cmd_invocar_feedback", {
        entregaId: entrega.id,
      });
      setFeedback(resultado);
    } catch (e) {
      setError(String(e));
    } finally {
      setCargando(null);
    }
  }

  return (
    <div style={{ border: "1px solid #99c", padding: "0.5rem", marginTop: "0.5rem" }}>
      <h4>
        Corrección — modo Asistente
        {entrega.alumno_nombre ? ` — ${entrega.alumno_nombre}` : ""}
      </h4>

      <button onClick={invocarEvidencias} disabled={cargando !== null}>
        {cargando === "evidencias" ? "Consultando IA…" : "1. Localizar evidencia (sin nota)"}
      </button>

      {criterios.map((c) => {
        const evidencia = evidencias.find((e) => e.criterio_id === c.codigo);
        return (
          <div key={c.id} style={{ marginTop: "0.5rem" }}>
            <strong>
              {c.codigo} ({c.puntuacion_max} pts): {c.descripcion}
            </strong>
            {evidencia && (
              <ul>
                {evidencia.evidencia_textual.map((cita, i) => (
                  <li key={i}>
                    <em>"{cita}"</em>
                  </li>
                ))}
              </ul>
            )}
            <label>
              Tu puntuación:{" "}
              <input
                type="number"
                min={0}
                max={c.puntuacion_max}
                step={0.5}
                value={puntuaciones[c.codigo] ?? ""}
                onChange={(e) =>
                  setPuntuaciones((prev) => ({ ...prev, [c.codigo]: e.currentTarget.value }))
                }
              />
            </label>
          </div>
        );
      })}

      <div style={{ marginTop: "0.5rem" }}>
        <button onClick={guardarNotaTentativa} disabled={cargando !== null}>
          {cargando === "nota" ? "Guardando…" : "2. Guardar mi evaluación"}
        </button>
        <button onClick={invocarFeedback} disabled={cargando !== null}>
          {cargando === "feedback" ? "Consultando IA…" : "3. Generar feedback y comprobar inconsistencias"}
        </button>
      </div>

      {feedback && (
        <div style={{ marginTop: "0.5rem" }}>
          <p>
            <strong>Feedback para el alumno:</strong> {feedback.comentario_feedback}
          </p>
          {feedback.inconsistencias.length > 0 && (
            <>
              <strong style={{ color: "darkorange" }}>Posibles inconsistencias:</strong>
              <ul>
                {feedback.inconsistencias.map((inc, i) => (
                  <li key={i}>
                    [{inc.criterio_id}] {inc.observacion}
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}

      <div style={{ marginTop: "0.5rem" }}>
        <button onClick={confirmarNota} disabled={cargando !== null}>
          {cargando === "confirmar" ? "Confirmando…" : "4. Confirmar nota (checkpoint final, Hito 6)"}
        </button>
        {notaConfirmada !== null && (
          <span style={{ marginLeft: "0.5rem" }}>
            ✅ Nota total confirmada: <strong>{notaConfirmada}</strong>
          </span>
        )}
      </div>

      {error && <pre style={{ color: "crimson" }}>{error}</pre>}
    </div>
  );
}
