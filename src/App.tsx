import { useEffect, useState } from "react";
import "./App.css";
import { listen } from "@tauri-apps/api/event";
import { Audit } from "./screens/Audit";
import { Diagnostics } from "./screens/Diagnostics";
import { Submissions } from "./screens/Submissions";
import { Rubrics } from "./screens/Rubrics";

type Tab = "diagnostics" | "rubrics" | "submissions" | "audit";

function App() {
  const [tab, setTab] = useState<Tab>("submissions");

  const [closeError, setCloseError] = useState<string | null>(null);
  useEffect(() => {
    const subscription = listen<string>("database-close-error", event => setCloseError(event.payload));
    return () => { void subscription.then(unlisten => unlisten()); };
  }, []);

  return (
    <main className="container">
      <h1>Corregir — Phase A (synthetic data)</h1>
      {closeError && <p role="alert">{closeError}</p>}
      <nav className="row" aria-label="Main navigation">
        <button onClick={() => setTab("rubrics")} disabled={tab === "rubrics"}>
          Rubrics
        </button>
        <button onClick={() => setTab("submissions")} disabled={tab === "submissions"}>
          Submissions
        </button>
        <button onClick={() => setTab("audit")} disabled={tab === "audit"}>
          Audit
        </button>
        <button onClick={() => setTab("diagnostics")} disabled={tab === "diagnostics"}>
          Diagnostics
        </button>
      </nav>

      {tab === "rubrics" && <Rubrics />}
      {tab === "submissions" && <Submissions />}
      {tab === "audit" && <Audit />}
      {tab === "diagnostics" && <Diagnostics />}
    </main>
  );
}

export default App;
