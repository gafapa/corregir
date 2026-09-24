import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RUBRICAS_SINTETICAS, RubricaConCriterios } from "../lib/types";

/**
 * Hito 2: creación y listado de rúbricas contra la base de datos cifrada.
 * Verificación manual: crear las dos rúbricas sintéticas, cerrar y reabrir
 * la app (`pnpm tauri dev`), y comprobar que siguen apareciendo en la lista.
 */
export function Rubricas() {
  const [rubricas, setRubricas] = useState<RubricaConCriterios[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [cargando, setCargando] = useState(false);

  async function recargar() {
    setError(null);
    try {
      const lista = await invoke<RubricaConCriterios[]>("cmd_listar_rubricas");
      setRubricas(lista);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    recargar();
  }, []);

  async function crearRubricaSintetica(indice: number) {
    setCargando(true);
    setError(null);
    try {
      await invoke<number>("cmd_crear_rubrica", {
        rubrica: RUBRICAS_SINTETICAS[indice],
      });
      await recargar();
    } catch (e) {
      setError(String(e));
    } finally {
      setCargando(false);
    }
  }

  return (
    <section>
      <h2>Rúbricas (Hito 2)</h2>
      <p>
        Datos de configuración (no son datos de alumnos): no requieren
        anonimización.
      </p>

      <div className="row">
        {RUBRICAS_SINTETICAS.map((r, i) => (
          <button key={r.titulo} onClick={() => crearRubricaSintetica(i)} disabled={cargando}>
            Crear rúbrica sintética: {r.titulo}
          </button>
        ))}
      </div>

      {error && <pre style={{ color: "crimson" }}>{error}</pre>}

      <h3>Rúbricas guardadas ({rubricas.length})</h3>
      <ul>
        {rubricas.map((r) => (
          <li key={r.id}>
            <strong>
              #{r.id} — {r.titulo}
            </strong>{" "}
            ({r.asignatura}, {r.curso})
            <ul>
              {r.criterios.map((c) => (
                <li key={c.id}>
                  {c.codigo}: {c.descripcion} ({c.puntuacion_max} pts)
                </li>
              ))}
            </ul>
          </li>
        ))}
      </ul>
    </section>
  );
}
