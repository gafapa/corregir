import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../lib/i18n";

type BackupStatus = { last_created_at: string | null; last_verified_at: string | null; needs_backup: boolean; integrity_ok: boolean | null };

export function BackupTools() {
  const { t, formatDateTime, errorText } = useI18n();
  const [status, setStatus] = useState<BackupStatus | null>(null);
  const [statusError, setStatusError] = useState("");
  async function refreshStatus() {
    try { setStatus(await invoke<BackupStatus>("cmd_backup_status")); setStatusError(""); }
    catch (cause) { setStatusError(t("backup.statusError", { error: errorText(cause) })); }
  }
  useEffect(() => {
    let active = true;
    const check = () => { void invoke<BackupStatus>("cmd_backup_status").then(result => { if (active) { setStatus(result); setStatusError(""); } }).catch(cause => { if (active) setStatusError(t("backup.statusError", { error: errorText(cause) })); }); };
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
    if (Array.from(password).length < 12 || password !== confirmation) { setError(t("backup.passwordRules")); return; }
    setBusy(true);
    try {
      const destination = await save({ defaultPath: t("backup.fileName"), filters: [{ name: t("backup.fileType"), extensions: ["corregirbackup"] }] });
      if (!destination) return;
      await invoke("cmd_export_backup", { destinationPath: destination, password });
      setNotice(t("backup.created", { path: destination }));
      await refreshStatus();
    } catch (cause) { setError(errorText(cause)); } finally { setBusy(false); setPassword(""); setConfirmation(""); }
  }
  async function chooseBackup() {
    setBusy(true); setError(null); setNotice("");
    try { const selected = await open({ multiple: false, filters: [{ name: t("backup.fileType"), extensions: ["corregirbackup"] }] }); if (typeof selected === "string") setSource(selected); }
    catch (cause) { setError(errorText(cause)); } finally { setBusy(false); }
  }
  async function restoreBackup() {
    if (!source) return;
    setBusy(true); setError(null); setNotice("");
    try {
      await invoke("cmd_restore_backup", { sourcePath: source, password });
      window.location.reload();
    } catch (cause) { setError(errorText(cause)); } finally { setBusy(false); setPassword(""); setConfirmation(""); }
  }
  async function verifyBackup() {
    if (!source) return;
    setBusy(true); setError(null); setNotice("");
    try {
      await invoke("cmd_verify_backup", { sourcePath: source, password });
      setNotice(t("backup.verified"));
      await refreshStatus();
    } catch (cause) { setError(errorText(cause)); } finally { setBusy(false); setPassword(""); setConfirmation(""); }
  }
  function formatDate(value: string | null) { return value ? formatDateTime(value) : t("backup.none"); }
  return <section className="backup-tools" aria-labelledby="backup-title">
    <h3 id="backup-title">{t("backup.title")}</h3>
    <p className="supporting-text">{t("backup.description")}</p>
    {status && <div className="backup-health" role="status">
      <p>{t("backup.health", { created: formatDate(status.last_created_at), verified: formatDate(status.last_verified_at) })}</p>
      <p>{status.integrity_ok === false ? t("backup.changed") : status.needs_backup ? t("backup.needed") : t("backup.matches")}</p>
    </div>}
    {statusError && <p role="status">{statusError}</p>}
    <p className="supporting-text">{t("backup.checkHelp")}</p>
    <fieldset disabled={busy}>
      <legend>{t("backup.legend")}</legend>
      <div className="form-grid">
        <label>{t("backup.password")}<input type="password" autoComplete="new-password" minLength={12} maxLength={1024} value={password} onChange={event => setPassword(event.currentTarget.value)} /></label>
        <label>{t("backup.repeat")}<input type="password" autoComplete="new-password" maxLength={1024} value={confirmation} onChange={event => setConfirmation(event.currentTarget.value)} /></label>
      </div>
      <div className="action-row"><button className="button-primary" onClick={() => void createBackup()} disabled={password.length < 12 || password !== confirmation}>{t("backup.create")}</button></div>
      <h4>{t("backup.restoreTitle")}</h4>
      <p className="supporting-text">{t("backup.restoreHelp")}</p>
      <div className="action-row"><button onClick={() => void chooseBackup()}>{t("backup.select")}</button><button onClick={() => void verifyBackup()} disabled={!source || password.length < 12}>{t("backup.verify")}</button><button onClick={() => void restoreBackup()} disabled={!source || password.length < 12}>{t("backup.restore")}</button></div>
      {source && <p className="supporting-text">{t("backup.selected", { path: source })}</p>}
    </fieldset>
    {busy && <p role="status">{t("backup.busy")}</p>}
    {notice && <p className="notice" role="status">{notice}</p>}
    {error && <pre role="alert">{error}</pre>}
  </section>;
}
