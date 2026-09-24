import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Hito 1: pantalla de diagnóstico temporal. Prueba el roundtrip completo
 * React -> Tauri (Rust) -> Ollama antes de construir el pipeline de
 * corrección encima. No se envía ningún dato de alumnos aquí.
 */
export function Diagnostico() {
  const [modelo, setModelo] = useState("qwen3:8b");
  const [respuesta, setRespuesta] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [cargando, setCargando] = useState(false);

  async function probarConexion() {
    setCargando(true);
    setError(null);
    setRespuesta(null);
    try {
      const resultado = await invoke<string>("cmd_probar_conexion_ia", {
        modelo,
      });
      setRespuesta(resultado);
    } catch (e) {
      setError(String(e));
    } finally {
      setCargando(false);
    }
  }

  return (
    <section>
      <h2>Diagnóstico (Hito 1)</h2>
      <p>Prueba de conexión con el servidor de inferencia local (Ollama).</p>

      <div className="row">
        <input
          id="modelo-input"
          value={modelo}
          onChange={(e) => setModelo(e.currentTarget.value)}
          placeholder="Nombre del modelo en Ollama"
        />
        <button onClick={probarConexion} disabled={cargando}>
          {cargando ? "Probando…" : "Probar conexión IA"}
        </button>
      </div>

      {respuesta && (
        <div>
          <h3>Respuesta del modelo:</h3>
          <pre>{respuesta}</pre>
        </div>
      )}

      {error && (
        <div>
          <h3>Error:</h3>
          <pre style={{ color: "crimson" }}>{error}</pre>
        </div>
      )}
    </section>
  );
}
