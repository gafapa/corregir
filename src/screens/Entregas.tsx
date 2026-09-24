import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { EntregaResumen, RubricaConCriterios } from "../lib/types";

/**
 * Hito 3: ingesta de entregas (PDF/imagen) y ejecución del pipeline de
 * extracción/OCR. Verificación manual: importar un PDF con texto nativo y
 * comprobar que el texto coincide; importar el PDF "escaneado" sintético y
 * comprobar que el método mostrado es OCR y el texto es reconocible.
 */
export function Entregas() {
  const [rubricas, setRubricas] = useState<RubricaConCriterios[]>([]);
  const [rubricaId, setRubricaId] = useState<number | null>(null);
  const [enunciadoId, setEnunciadoId] = useState<number | null>(null);
  const [entregas, setEntregas] = useState<EntregaResumen[]>([]);
  const [cargando, setCargando] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<RubricaConCriterios[]>("cmd_listar_rubricas").then(setRubricas);
  }, []);

  async function crearEnunciado() {
    if (rubricaId === null) return;
    setError(null);
    try {
      const id = await invoke<number>("cmd_crear_enunciado", {
        rubricaId,
        texto: "Enunciado de prueba (Hito 3) para la rúbrica seleccionada.",
        materialesRef: null,
      });
      setEnunciadoId(id);
      setEntregas([]);
    } catch (e) {
      setError(String(e));
    }
  }

  async function recargarEntregas(id: number) {
    const lista = await invoke<EntregaResumen[]>("cmd_listar_entregas", {
      enunciadoId: id,
    });
    setEntregas(lista);
  }

  async function importarArchivo() {
    if (enunciadoId === null) return;
    const seleccion = await open({
      multiple: false,
      filters: [{ name: "Documentos", extensions: ["pdf", "png", "jpg", "jpeg"] }],
    });
    if (!seleccion || Array.isArray(seleccion)) return;

    setCargando(true);
    setError(null);
    try {
      await invoke<number>("cmd_importar_entrega", {
        enunciadoId,
        rutaArchivo: seleccion,
      });
      await recargarEntregas(enunciadoId);
    } catch (e) {
      setError(String(e));
    } finally {
      setCargando(false);
    }
  }

  return (
    <section>
      <h2>Entregas (Hito 3)</h2>
      <p>
        Importa un PDF o una imagen. El texto se extrae de forma nativa si el
        PDF ya lo tiene, o mediante OCR local si no (p. ej. un PDF
        escaneado).
      </p>

      <div className="row">
        <select
          value={rubricaId ?? ""}
          onChange={(e) => setRubricaId(Number(e.currentTarget.value) || null)}
        >
          <option value="">Selecciona una rúbrica…</option>
          {rubricas.map((r) => (
            <option key={r.id} value={r.id}>
              #{r.id} — {r.titulo}
            </option>
          ))}
        </select>
        <button onClick={crearEnunciado} disabled={rubricaId === null}>
          Crear enunciado de prueba
        </button>
      </div>

      {enunciadoId !== null && (
        <>
          <p>Enunciado activo: #{enunciadoId}</p>
          <button onClick={importarArchivo} disabled={cargando}>
            {cargando ? "Procesando…" : "Importar PDF/imagen…"}
          </button>
        </>
      )}

      {error && <pre style={{ color: "crimson" }}>{error}</pre>}

      <h3>Entregas procesadas ({entregas.length})</h3>
      <ul>
        {entregas.map((e) => (
          <li key={e.id}>
            <strong>
              #{e.id} — método: {e.metodo_ocr ?? "?"} — estado: {e.estado_pipeline}
            </strong>
            <pre style={{ whiteSpace: "pre-wrap" }}>{e.texto_ocr}</pre>
          </li>
        ))}
      </ul>
    </section>
  );
}
