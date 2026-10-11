import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

export function BackupTools() {
  const [status, setStatus] = useState<{ last_created_at: string | null; last_verified_at: string | null; needs_backup: boolean; integrity_ok: boolean | null } | null>(null);
  const [statusError, setStatusError] = useState("");
  async function refreshStatus() {
    try { setStatus(await invoke("cmd_backup_status")); setStatusError(""); }
    catch (error) { setStatusError(`Could not check backup health: ${String(error)}`); }
  }
  useEffect(() => {
    let active = true;
    const check = () => { void invoke<typeof status>("cmd_backup_status").then(result => { if (active) { setStatus(result); setStatusError(""); } }).catch(error => { if (active) setStatusError(`Could not check backup health: ${String(error)}`); }); };
    check();
    const interval = window.setInterval(check, 86400000);
    return () => { active = false; window.clearInterval(interval); };
  }, []);
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [source, setSource] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  async function createBackup() {
    setError(null); setNotice("");
    if (Array.from(password).length < 12 || password !== confirmation) { setError("Use at least 12 characters and repeat the same password."); return; }
    setBusy(true);
    try {
      const destination = await save({ defaultPath: "workspace.corregirbackup", filters: [{ name: "Encrypted workspace", extensions: ["corregirbackup"] }] });
      if (!destination) return;
      await invoke("cmd_export_backup", { destinationPath: destination, password });
      setNotice(`Encrypted backup saved to ${destination}. Keep its password separately; Windows credentials are not needed to restore it.`);
      await refreshStatus();
    } catch (error) { setError(String(error)); } finally { setBusy(false); setPassword(""); setConfirmation(""); }
  }
  async function chooseBackup() {
    setBusy(true); setError(null); setNotice("");
    try { const selected = await open({ multiple: false, filters: [{ name: "Encrypted workspace", extensions: ["corregirbackup"] }] }); if (typeof selected === "string") setSource(selected); }
    catch (error) { setError(String(error)); } finally { setBusy(false); }
  }
  async function restoreBackup() {
    if (!source) return;
    setBusy(true); setError(null); setNotice("");
    try {
      await invoke("cmd_restore_backup", { sourcePath: source, password });
      window.location.reload();
    } catch (error) { setError(String(error)); } finally { setBusy(false); setPassword(""); setConfirmation(""); }
  }
  async function verifyBackup() {
    if (!source) return;
    setBusy(true); setError(null); setNotice("");
    try {
      await invoke("cmd_verify_backup", { sourcePath: source, password });
      setNotice("Recovery check passed: the backup decrypted and its database schema and references are valid. Your current records were not replaced.");
      await refreshStatus();
    } catch (error) { setError(String(error)); } finally { setBusy(false); setPassword(""); setConfirmation(""); }
  }
  function formatDate(value: string | null) { return value ? new Date(value).toLocaleString() : "None recorded"; }
  return <section className="backup-tools" aria-labelledby="backup-title">
    <h3 id="backup-title">Backup and recovery</h3>
    <p className="supporting-text">A portable backup contains your rubric, assignment, identity and grading records, encrypted with its own password. Original documents are not included. Limit: 128 MiB.</p>
    {status && <div className="backup-health" role="status">
      <p>Last backup created: {formatDate(status.last_created_at)}. Last recovery check: {formatDate(status.last_verified_at)}.</p>
      <p>{status.integrity_ok === false ? "The recorded backup is missing or has changed. Create a new backup or verify another copy." : status.needs_backup ? "Create a fresh backup. A reminder appears when no verified copy is recorded or the last copy is at least seven days old." : "The recorded backup matches its verified fingerprint."}</p>
    </div>}
    {statusError && <p role="status">{statusError}</p>}
    <p className="supporting-text">File integrity is checked when this screen opens and daily while it stays open. Passwords are never saved; a recovery check requires you to enter the backup password.</p>
    <fieldset disabled={busy}>
      <legend>Portable encrypted workspace</legend>
      <div className="form-grid">
        <label>Backup password<input type="password" autoComplete="new-password" minLength={12} maxLength={1024} value={password} onChange={event => setPassword(event.currentTarget.value)} /></label>
        <label>Repeat password to create a backup<input type="password" autoComplete="new-password" maxLength={1024} value={confirmation} onChange={event => setConfirmation(event.currentTarget.value)} /></label>
      </div>
      <div className="action-row"><button className="button-primary" onClick={createBackup} disabled={password.length < 12 || password !== confirmation}>Create encrypted backup</button></div>
      <h4>Restore into an empty workspace</h4>
      <p className="supporting-text">Select a backup and enter its password above. Restore is refused when the current workspace contains rubrics, submissions or audit events; existing work is never replaced.</p>
      <div className="action-row"><button onClick={chooseBackup}>Select backup</button><button onClick={verifyBackup} disabled={!source || password.length < 12}>Check recovery without restoring</button><button onClick={restoreBackup} disabled={!source || password.length < 12}>Restore selected backup</button></div>
      {source && <p className="supporting-text">Selected: {source}</p>}
    </fieldset>
    {busy && <p role="status">Processing encrypted backup…</p>}
    {notice && <p className="notice" role="status">{notice}</p>}
    {error && <pre role="alert">{error}</pre>}
  </section>;
}
