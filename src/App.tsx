import { useEffect, useState } from "react";
import "./App.css";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { RecoveryPanel } from "./screens/RecoveryPanel";
import { Audit } from "./screens/Audit";
import { Diagnostics } from "./screens/Diagnostics";
import { Submissions } from "./screens/Submissions";
import { Rubrics } from "./screens/Rubrics";
import { Classes } from "./screens/Classes";
import { Privacy } from "./screens/Privacy";
import { WorkDirectory, type WorkspaceStatus } from "./screens/WorkDirectory";
import { LANGUAGES, useI18n, type Language, type MessageKey } from "./lib/i18n";
import type { RetentionDue } from "./lib/types";

type Tab = "submissions" | "classes" | "rubrics" | "privacy" | "audit" | "diagnostics";
const TABS: { id: Tab; label: MessageKey }[] = [
  { id: "submissions", label: "nav.submissions" },
  { id: "classes", label: "nav.classes" },
  { id: "rubrics", label: "nav.rubrics" },
  { id: "privacy", label: "nav.privacy" },
  { id: "audit", label: "nav.audit" },
  { id: "diagnostics", label: "nav.settings" },
];

function App() {
  const { t, language, setLanguage } = useI18n();
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
  const [retentionDue, setRetentionDue] = useState(0);
  const [workspace, setWorkspace] = useState<WorkspaceStatus | null>(null);
  const lockedWorkspace = !!workspace?.portable && !workspace.opened;
  const ready = !startupLoading && !startupError && !recoveryReason && !lockedWorkspace;
  useEffect(() => {
    if (!ready) return;
    let active = true;
    const check = () => {
      void invoke<{ needs_backup: boolean }>("cmd_backup_status").then(status => { if (active) setBackupDue(status.needs_backup); }).catch(() => { if (active) setBackupDue(true); });
      void invoke<RetentionDue[]>("cmd_retention_status").then(due => { if (active) setRetentionDue(due.length); }).catch(() => { if (active) setRetentionDue(0); });
    };
    check();
    const interval = window.setInterval(check, 86400000);
    return () => { active = false; window.clearInterval(interval); };
  }, [ready, tab]);
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
    const subscription = listen<string>("database-close-error", () => setCloseError(t("app.closeError")));
    return () => { void subscription.then(unlisten => unlisten()); };
  }, [t]);

  return (
    <div className="app-shell">
      <a className="skip-link" href="#workspace">{t("app.skip")}</a>
      <header className="app-header">
        <div className="brand">
          <svg className="brand-mark" viewBox="0 0 32 32" fill="none" aria-hidden="true">
            <rect x="1" y="1" width="30" height="30" rx="8" fill="currentColor" />
            <path d="m9 16 5 5 9-10" stroke="var(--surface)" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          <h1>Corregir</h1>
          <span className="brand-description">{t("app.tagline")}</span>
        </div>
        <div className="header-tools">
          <span className="environment-label">{t("app.environment")}</span>
          <label className="language-picker">
            <span className="visually-hidden">{t("app.language")}</span>
            <select value={language} aria-label={t("app.language")} onChange={event => setLanguage(event.currentTarget.value as Language)}>
              {LANGUAGES.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
            </select>
          </label>
        </div>
      </header>
      <nav className="main-navigation" aria-label={t("app.navigation")}>
        {TABS.map(item => (
          <button key={item.id} disabled={!ready || hasTextDraft} aria-current={tab === item.id ? "page" : undefined} onClick={() => setTab(item.id)}>
            {t(item.label)}
          </button>
        ))}
      </nav>
      <main id="workspace" className="container" tabIndex={-1}>
        {closeError && <p role="alert">{closeError}</p>}

        {startupLoading ? <p role="status">{t("app.opening")}</p> : startupError ? <p role="alert">{t("app.startupError", { error: startupError })}</p> : lockedWorkspace && workspace ? <WorkDirectory initialDirectory={workspace.directory} onOpened={() => window.location.reload()} /> : recoveryReason ? <RecoveryPanel reason={recoveryReason} /> : <>
          {workspace && <p className="workspace-location">{t("app.workDirectory", { directory: workspace.directory })}</p>}
          {backupDue && tab !== "diagnostics" && <div className="banner"><p role="status">{t("app.backupDue")}</p><button disabled={hasTextDraft} onClick={() => setTab("diagnostics")}>{t("app.openBackups")}</button></div>}
          {retentionDue > 0 && tab !== "privacy" && <div className="banner"><p role="status">{t("app.retentionDue", { count: retentionDue })}</p><button disabled={hasTextDraft} onClick={() => setTab("privacy")}>{t("app.openPrivacy")}</button></div>}
          {tab === "submissions" && <Submissions />}
          {tab === "classes" && <Classes />}
          {tab === "rubrics" && <Rubrics />}
          {tab === "privacy" && <Privacy />}
          {tab === "audit" && <Audit />}
          {tab === "diagnostics" && <Diagnostics />}
        </>}
      </main>
      <footer className="app-footer"><span>{t("app.footerReview")}</span><span>{t("app.footerLocal")}</span></footer>
    </div>
  );
}

export default App;
