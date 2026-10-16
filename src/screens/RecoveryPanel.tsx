import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../lib/i18n";

export function RecoveryPanel({ reason }: { reason: string }) {
  const { t, errorText } = useI18n();
  const [source, setSource] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function chooseBackup() {
    setBusy(true); setError(null);
    try {
      const path = await open({ multiple: false, filters: [{ name: t("backup.fileType"), extensions: ["corregirbackup"] }] });
      if (typeof path === "string") setSource(path);
    } catch (cause) { setError(errorText(cause)); }
    finally { setBusy(false); }
  }
  async function recover() {
    setBusy(true); setError(null);
    try {
      await invoke("cmd_recover_workspace", { sourcePath: source, password });
      window.location.reload();
    } catch (cause) { setError(errorText(cause)); }
    finally { setBusy(false); setPassword(""); }
  }
  return <section>
    <h2>{t("recovery.title")}</h2>
    <p role="alert">{errorText(reason)}</p>
    <p>{t("recovery.description")}</p>
    <fieldset disabled={busy}>
      <legend>{t("recovery.legend")}</legend>
      <button type="button" onClick={() => void chooseBackup()}>{t("recovery.choose")}</button>
      {source && <p className="path-label">{source}</p>}
      <label htmlFor="recovery-password">{t("backup.password")}</label>
      <input id="recovery-password" type="password" autoComplete="current-password" value={password} onChange={event => setPassword(event.target.value)} />
      <div className="action-row"><button type="button" className="button-primary" disabled={!source || [...password].length < 12} onClick={() => void recover()}>{t("recovery.recover")}</button></div>
    </fieldset>
    {busy && <p role="status">{t("recovery.busy")}</p>}
    {error && <p role="alert">{error}</p>}
    <p>{t("recovery.warning")}</p>
  </section>;
}
