import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CandidatoIdentificador, EntregaResumen } from "../lib/types";

/**
 * Hito 4: pantalla de revisión humana obligatoria de la anonimización.
 * Ningún texto llega a la IA sin pasar por esta confirmación explícita —
 * ver `pipeline::anonimizacion::TextoAnonimizadoConfirmado` en el backend.
 */
export function PanelAnonimizacion({
  entrega,
  onConfirmado,
}: {
  entrega: EntregaResumen;
  onConfirmado: () => void;
}) {
  const [roster, setRoster] = useState("");
  const [candidatos, setCandidatos] = useState<CandidatoIdentificador[]>([]);
  const [aceptados, setAceptados] = useState<Set<number>>(new Set());
  const [manual, setManual] = useState("");
  const [alumnoNombre, setAlumnoNombre] = useState("");
  const [resultado, setResultado] = useState<{ alias: string } | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function detectar() {
    setError(null);
    try {
      const nombres = roster
        .split(/[\n,]/)
        .map((n) => n.trim())
        .filter(Boolean);
      const lista = await invoke<CandidatoIdentificador[]>("cmd_detectar_identificadores", {
        entregaId: entrega.id,
        roster: nombres,
      });
      setCandidatos(lista);
      setAceptados(new Set(lista.map((_, i) => i)));
    } catch (e) {
      setError(String(e));
    }
  }

  function alternar(indice: number) {
    setAceptados((prev) => {
      const copia = new Set(prev);
      if (copia.has(indice)) copia.delete(indice);
      else copia.add(indice);
      return copia;
    });
  }

  function resolverSpansManuales(): [number, number][] {
    const texto = entrega.texto_ocr ?? "";
    const spans: [number, number][] = [];
    for (const linea of manual.split("\n").map((l) => l.trim()).filter(Boolean)) {
      let desde = 0;
      let pos;
      while ((pos = texto.indexOf(linea, desde)) !== -1) {
        spans.push([pos, pos + linea.length]);
        desde = pos + linea.length;
      }
    }
    return spans;
  }

  async function confirmar() {
    if (!alumnoNombre.trim()) {
      setError("Indica el nombre real del alumno (queda solo en local, nunca se exporta).");
      return;
    }
    setError(null);
    try {
      const spans: [number, number][] = [
        ...candidatos.filter((_, i) => aceptados.has(i)).map((c): [number, number] => [c.inicio, c.fin]),
        ...resolverSpansManuales(),
      ];
      const alias = await invoke<string>("cmd_confirmar_anonimizacion", {
        entregaId: entrega.id,
        spans,
        alumnoNombre,
        alumnoIdClase: null,
      });
      setResultado({ alias });
      onConfirmado();
    } catch (e) {
      setError(String(e));
    }
  }

  if (entrega.estado_pipeline === "anonimizada") {
    return <p>✅ Ya anonimizada.</p>;
  }

  return (
    <div style={{ border: "1px solid #ccc", padding: "0.5rem", marginTop: "0.5rem" }}>
      <h4>Anonimizar entrega #{entrega.id}</h4>

      <label>
        Lista de la clase (un nombre completo por línea o separados por comas):
        <textarea
          value={roster}
          onChange={(e) => setRoster(e.currentTarget.value)}
          rows={2}
          style={{ width: "100%" }}
        />
      </label>
      <button onClick={detectar}>Detectar identificadores</button>

      {candidatos.length > 0 && (
        <>
          <h5>Candidatos detectados (revisa antes de confirmar):</h5>
          <ul>
            {candidatos.map((c, i) => (
              <li key={i}>
                <label>
                  <input
                    type="checkbox"
                    checked={aceptados.has(i)}
                    onChange={() => alternar(i)}
                  />{" "}
                  [{c.tipo}] "{c.texto}"
                </label>
              </li>
            ))}
          </ul>
        </>
      )}

      <label>
        Texto adicional a redactar manualmente (un fragmento exacto por línea):
        <textarea
          value={manual}
          onChange={(e) => setManual(e.currentTarget.value)}
          rows={2}
          style={{ width: "100%" }}
        />
      </label>

      <label>
        Nombre real del alumno (solo local, nunca se envía a la IA):
        <input value={alumnoNombre} onChange={(e) => setAlumnoNombre(e.currentTarget.value)} />
      </label>

      <button onClick={confirmar}>Confirmar anonimización</button>

      {error && <pre style={{ color: "crimson" }}>{error}</pre>}
      {resultado && (
        <p>
          ✅ Confirmada. Alias asignado: <code>{resultado.alias}</code>
        </p>
      )}
    </div>
  );
}
