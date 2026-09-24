import { useState } from "react";
import "./App.css";
import { Diagnostico } from "./screens/Diagnostico";
import { Entregas } from "./screens/Entregas";
import { Rubricas } from "./screens/Rubricas";

type Pestana = "diagnostico" | "rubricas" | "entregas";

function App() {
  const [pestana, setPestana] = useState<Pestana>("rubricas");

  return (
    <main className="container">
      <h1>Corregir — Fase A (datos sintéticos)</h1>
      <nav className="row">
        <button onClick={() => setPestana("rubricas")} disabled={pestana === "rubricas"}>
          Rúbricas
        </button>
        <button onClick={() => setPestana("entregas")} disabled={pestana === "entregas"}>
          Entregas
        </button>
        <button onClick={() => setPestana("diagnostico")} disabled={pestana === "diagnostico"}>
          Diagnóstico
        </button>
      </nav>

      {pestana === "rubricas" && <Rubricas />}
      {pestana === "entregas" && <Entregas />}
      {pestana === "diagnostico" && <Diagnostico />}
    </main>
  );
}

export default App;
