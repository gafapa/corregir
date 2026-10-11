import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export type WorkspaceStatus = { portable: boolean; directory: string; opened: boolean };
type DirectoryStatus = { initialized: boolean; windows_bound: boolean };

export function WorkDirectory({ initialDirectory, onOpened }: { initialDirectory: string; onOpened: () => void }) {
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
    void invoke<DirectoryStatus>("cmd_inspect_work_directory", { directory }).then(result => { if (current) setStatus(result); }).catch(cause => { if (current) setError(String(cause)); });
    return () => { current = false; };
  }, [directory]);
  async function chooseDirectory() {
    setBusy(true); setError(null);
    try {
      const path = await open({ directory: true, multiple: false, title: "Choose work directory", defaultPath: directory });
      if (typeof path === "string") setDirectory(path);
    } catch (cause) { if (active.current) setError(String(cause)); }
    finally { if (active.current) setBusy(false); }
  }
  async function unlock(event: React.FormEvent) {
    event.preventDefault(); setBusy(true); setError(null);
    const secret = password; setPassword(""); setConfirmation("");
    try {
      await invoke("cmd_open_work_directory", { directory, password: secret });
      if (active.current) onOpened();
    } catch (cause) { if (active.current) setError(String(cause)); }
    finally { if (active.current) setBusy(false); }
  }
  const creating = status !== null && !status.initialized;
  const valid = !!status && !status.windows_bound && [...password].length >= 12 && (!creating || password === confirmation);
  return <section className="work-directory" aria-labelledby="work-directory-title">
    <h2 id="work-directory-title">Open your work directory</h2>
    <p>Keep assessments in a folder you choose. Its encrypted database and protected key can travel together to another computer.</p>
    <form onSubmit={unlock} aria-busy={busy}>
      <label>Work directory<input value={directory} readOnly /></label>
      <button type="button" disabled={busy} onClick={chooseDirectory}>Choose directory</button>
      {!status && !error && <p role="status">Checking work directory…</p>}
      {status?.windows_bound ? <p role="alert">This folder contains a Windows-bound or legacy workspace. Choose a new directory, then restore a verified password-protected backup. Existing files have not been replaced.</p> : status && <>
        <p>{creating ? "Create a password for this new work directory." : "Enter this work directory’s password to unlock it."}</p>
        <label>Work-directory password<input type="password" autoComplete={creating ? "new-password" : "current-password"} value={password} maxLength={1024} disabled={busy} onChange={event => setPassword(event.currentTarget.value)} aria-describedby="workspace-password-help" /></label>
        <p id="workspace-password-help" className="supporting-text">Use at least 12 characters. The password is never saved. Keep it separately: a forgotten password requires restoring a verified backup into a new directory.</p>
        {creating && <label>Confirm work-directory password<input type="password" autoComplete="new-password" value={confirmation} maxLength={1024} disabled={busy} onChange={event => setConfirmation(event.currentTarget.value)} /></label>}
        <button className="button-primary" type="submit" disabled={busy || !valid}>{busy ? "Opening work directory…" : creating ? "Create work directory" : "Unlock work directory"}</button>
      </>}
      {error && <p role="alert">{error}</p>}
    </form>
    <p className="supporting-text">The default is the “work” folder beside the portable application. Close Corregir before copying this folder or choosing another directory. Original documents remain where you selected them.</p>
  </section>;
}
