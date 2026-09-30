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
    <div className="app-shell">
      <a className="skip-link" href="#workspace">Skip to workspace</a>
      <header className="app-header">
        <div className="brand">
          <svg className="brand-mark" viewBox="0 0 32 32" fill="none" aria-hidden="true">
            <rect x="1" y="1" width="30" height="30" rx="8" fill="currentColor" />
            <path d="m9 16 5 5 9-10" stroke="var(--surface)" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          <h1>Corregir</h1>
          <span className="brand-description">Grading workspace</span>
        </div>
        <span className="environment-label">Phase A · Synthetic data</span>
      </header>
      <nav className="main-navigation" aria-label="Main navigation">
        {(["submissions", "rubrics", "audit", "diagnostics"] as Tab[]).map((item) => (
          <button key={item} aria-current={tab === item ? "page" : undefined} onClick={() => setTab(item)}>
            {item === "submissions" ? "Submissions" : item === "rubrics" ? "Rubrics" : item === "audit" ? "Audit" : "Diagnostics"}
          </button>
        ))}
      </nav>
      <main id="workspace" className="container" tabIndex={-1}>
        {closeError && <p role="alert">{closeError}</p>}

        {tab === "rubrics" && <Rubrics />}
        {tab === "submissions" && <Submissions />}
        {tab === "audit" && <Audit />}
        {tab === "diagnostics" && <Diagnostics />}
      </main>
      <footer className="app-footer"><span>Teacher-reviewed grading assistance</span><span>Local desktop workspace</span></footer>
    </div>
  );
}

export default App;
