import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../lib/i18n";

export type WorkspaceStatus = { portable: boolean; directory: string; opened: boolean };
type DirectoryStatus = { initialized: boolean; windows_bound: boolean };

export function WorkDirectory({ initialDirectory, onOpened }: { initialDirectory: string; onOpened: () => void }) {
  const { t, errorText } = useI18n();
  const [directory, setDirectory] = useState(initialDirectory);
  const [status, setStatus] = useState<DirectoryStatus | null>(null);
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const active = useRef(true);
  useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  useEffect(() => {
    let current = true; setStatus(null); setPassword(""); setConfirmation(""); setError(null);
    void invoke<DirectoryStatus>("cmd_inspect_work_directory", { directory }).then(result => { if (current) setStatus(result); }).catch(cause => { if (current) setError(errorText(cause)); });
    return () => { current = false; };
  }, [directory]);
  async function chooseDirectory() {
    setBusy(true); setError(null);
    try {
      const path = await open({ directory: true, multiple: false, title: t("work.choose"), defaultPath: directory });
      if (typeof path === "string") setDirectory(path);
    } catch (cause) { if (active.current) setError(errorText(cause)); }
    finally { if (active.current) setBusy(false); }
  }
  async function unlock(event: React.FormEvent) {
    event.preventDefault(); setBusy(true); setError(null);
    const secret = password; setPassword(""); setConfirmation("");
    try {
      await invoke("cmd_open_work_directory", { directory, password: secret });
      if (active.current) onOpened();
    } catch (cause) { if (active.current) setError(errorText(cause)); }
    finally { if (active.current) setBusy(false); }
  }
  const creating = status !== null && !status.initialized;
  const valid = !!status && !status.windows_bound && [...password].length >= 12 && (!creating || password === confirmation);
  return <section className="work-directory" aria-labelledby="work-directory-title">
    <h2 id="work-directory-title">{t("work.title")}</h2>
    <p>{t("work.description")}</p>
    <form onSubmit={event => void unlock(event)} aria-busy={busy}>
      <label>{t("work.directory")}<input value={directory} readOnly /></label>
      <button type="button" disabled={busy} onClick={() => void chooseDirectory()}>{t("work.chooseButton")}</button>
      {!status && !error && <p role="status">{t("work.checking")}</p>}
      {status?.windows_bound ? <p role="alert">{t("work.windowsBound")}</p> : status && <>
        <p>{creating ? t("work.createPrompt") : t("work.unlockPrompt")}</p>
        <label>{t("work.password")}<input type="password" autoComplete={creating ? "new-password" : "current-password"} value={password} maxLength={1024} disabled={busy} onChange={event => setPassword(event.currentTarget.value)} aria-describedby="workspace-password-help" /></label>
        <p id="workspace-password-help" className="supporting-text">{t("work.passwordHelp")}</p>
        {creating && <label>{t("work.confirm")}<input type="password" autoComplete="new-password" value={confirmation} maxLength={1024} disabled={busy} onChange={event => setConfirmation(event.currentTarget.value)} /></label>}
        <button className="button-primary" type="submit" disabled={busy || !valid}>{busy ? t("work.opening") : creating ? t("work.create") : t("work.unlock")}</button>
      </>}
      {error && <p role="alert">{error}</p>}
    </form>
    <p className="supporting-text">{t("work.footer")}</p>
  </section>;
}
