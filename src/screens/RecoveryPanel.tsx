import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export function RecoveryPanel({ reason }: { reason: string }) {
  const [source, setSource] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function chooseBackup() {
    setBusy(true); setError(null);
    try {
      const path = await open({ multiple: false, filters: [{ name: "Encrypted workspace", extensions: ["corregirbackup"] }] });
      if (typeof path === "string") setSource(path);
    } catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  }
  async function recover() {
    setBusy(true); setError(null);
    try {
      await invoke("cmd_recover_workspace", { sourcePath: source, password });
      window.location.reload();
    } catch (cause) { setError(String(cause)); }
    finally { setBusy(false); setPassword(""); }
  }
  return <section>
    <h2>Recover your workspace</h2>
    <p role="alert">{reason}</p>
    <p>Choose an encrypted backup and enter its password. Recovery restores the records present when that backup was created. The original encrypted file is preserved before replacement.</p>
    <fieldset disabled={busy}>
      <legend>Encrypted backup</legend>
      <button type="button" onClick={() => void chooseBackup()}>Choose backup</button>
      {source && <p className="path-label">{source}</p>}
      <label htmlFor="recovery-password">Backup password</label>
      <input id="recovery-password" type="password" autoComplete="current-password" value={password} onChange={event => setPassword(event.target.value)} />
      <div className="action-row"><button type="button" className="button-primary" disabled={!source || [...password].length < 12} onClick={() => void recover()}>Recover and preserve original file</button></div>
    </fieldset>
    {busy && <p role="status">Recovering workspace…</p>}
    {error && <p role="alert">{error}</p>}
    <p>Without the original device credential or a previously created backup and its password, the encrypted records cannot be recovered.</p>
  </section>;
}
