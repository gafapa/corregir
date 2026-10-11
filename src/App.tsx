import { useEffect, useState } from "react";
import "./App.css";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { RecoveryPanel } from "./screens/RecoveryPanel";
import { Audit } from "./screens/Audit";
import { Diagnostics } from "./screens/Diagnostics";
import { Submissions } from "./screens/Submissions";
import { Rubrics } from "./screens/Rubrics";
import { WorkDirectory, type WorkspaceStatus } from "./screens/WorkDirectory";

type Tab = "diagnostics" | "rubrics" | "submissions" | "audit";

function App() {
  const [hasTextDraft, setHasTextDraft] = useState(false);
  useEffect(() => {
    const handleDraft = (event: Event) => setHasTextDraft(Boolean((event as CustomEvent<boolean>).detail));
    window.addEventListener("corregir-text-draft", handleDraft);
    return () => window.removeEventListener("corregir-text-draft", handleDraft);
  }, []);
  const [tab, setTab] = useState<Tab>("submissions");
  const [startupLoading, setStartupLoading] = useState(true);
  const [recoveryReason, setRecoveryReason] = useState<string | null>(null);
  const [startupError, setStartupError] = useState<string | null>(null);
  const [backupDue, setBackupDue] = useState(false);
  const [workspace, setWorkspace] = useState<WorkspaceStatus | null>(null);
  const lockedWorkspace = !!workspace?.portable && !workspace.opened;
  useEffect(() => {
    if (startupLoading || startupError || recoveryReason || lockedWorkspace) return;
    let active = true;
    const check = () => { void invoke<{ needs_backup: boolean }>("cmd_backup_status").then(status => { if (active) setBackupDue(status.needs_backup); }).catch(() => { if (active) setBackupDue(true); }); };
    check();
    const interval = window.setInterval(check, 86400000);
    return () => { active = false; window.clearInterval(interval); };
  }, [startupLoading, startupError, recoveryReason, lockedWorkspace, tab]);
  useEffect(() => {
    let active = true;
    void invoke<WorkspaceStatus>("cmd_workspace_status").then(async status => {
      if (active) setWorkspace(status);
      const reason = await invoke<string | null>("cmd_recovery_status");
      if (active) setRecoveryReason(reason);
    }).catch(error => { if (active) setStartupError(String(error)); })
      .finally(() => { if (active) setStartupLoading(false); });
    return () => { active = false; };
  }, []);

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
          <button key={item} disabled={startupLoading || lockedWorkspace || !!recoveryReason || !!startupError || hasTextDraft} aria-current={tab === item ? "page" : undefined} onClick={() => setTab(item)}>
            {item === "submissions" ? "Submissions" : item === "rubrics" ? "Rubrics" : item === "audit" ? "Audit" : "Diagnostics"}
          </button>
        ))}
      </nav>
      <main id="workspace" className="container" tabIndex={-1}>
        {closeError && <p role="alert">{closeError}</p>}

        {startupLoading ? <p role="status">Opening workspace…</p> : startupError ? <p role="alert">Cannot open the workspace: {startupError}. Restart the application to retry.</p> : lockedWorkspace && workspace ? <WorkDirectory initialDirectory={workspace.directory} onOpened={() => window.location.reload()} /> : recoveryReason ? <RecoveryPanel reason={recoveryReason} /> : <>
          {workspace && <p className="workspace-location">Work directory: {workspace.directory}</p>}
          {backupDue && tab !== "diagnostics" && <div className="action-row"><p role="status">Check your backup: no recent verified copy is available.</p><button disabled={hasTextDraft} onClick={() => setTab("diagnostics")}>Open backup tools</button></div>}
          {tab === "rubrics" && <Rubrics />}
          {tab === "submissions" && <Submissions />}
          {tab === "audit" && <Audit />}
          {tab === "diagnostics" && <Diagnostics />}
        </>}
      </main>
      <footer className="app-footer"><span>Teacher-reviewed grading assistance</span><span>Local desktop workspace</span></footer>
    </div>
  );
}

export default App;
