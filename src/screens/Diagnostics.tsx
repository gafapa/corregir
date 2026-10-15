import { useEffect, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BackupTools } from "./BackupTools";
import { LANGUAGES, useI18n, type Language } from "../lib/i18n";
import type { Preferences } from "../lib/types";
type OllamaSettings = { url: string; model: string };

export function Diagnostics() {
  const { t, language, setLanguage, errorText } = useI18n();
  const [model, setModel] = useState("qwen3:8b");
  const [url, setUrl] = useState("http://127.0.0.1:11434");
  const [saved, setSaved] = useState<OllamaSettings | null>(null);
  const [preferences, setPreferences] = useState<Preferences | null>(null);
  const [response, setResponse] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let active = true;
    Promise.all([
      invoke<OllamaSettings>("cmd_load_ollama_settings"),
      invoke<Preferences>("cmd_load_preferences").catch(() => null),
    ]).then(([settings, loadedPreferences]) => {
      if (!active) return;
      setUrl(settings.url); setModel(settings.model); setSaved(settings);
      setPreferences(loadedPreferences ?? { feedback_language: "auto", retention_days: null });
    }).catch(cause => { if (active) setError(errorText(cause)); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, []);
  async function saveSettings(event: FormEvent) {
    event.preventDefault(); setLoading(true); setError(null); setNotice("");
    const settings = { url: url.trim(), model: model.trim() };
    try { await invoke("cmd_save_ollama_settings", { settings }); setUrl(settings.url); setModel(settings.model); setSaved(settings); setNotice(t("settings.saved")); }
    catch (cause) { setError(errorText(cause)); } finally { setLoading(false); }
  }
  async function saveFeedbackLanguage(value: Preferences["feedback_language"]) {
    if (!preferences) return;
    const next = { ...preferences, feedback_language: value };
    setLoading(true); setError(null); setNotice("");
    try { await invoke("cmd_save_preferences", { preferences: next }); setPreferences(next); setNotice(t("settings.feedbackLanguageSaved")); }
    catch (cause) { setError(errorText(cause)); } finally { setLoading(false); }
  }
  async function testConnection() {
    setLoading(true); setError(null); setResponse(null); setNotice("");
    try { setResponse(await invoke<string>("cmd_test_ai_connection", { model: model.trim(), url: url.trim() })); }
    catch (cause) { setError(errorText(cause)); } finally { setLoading(false); }
  }
  const dirty = saved?.url !== url.trim() || saved?.model !== model.trim();
  return <section aria-labelledby="diagnostics-title">
    <h2 id="diagnostics-title">{t("settings.title")}</h2>
    <p className="page-description">{t("settings.description")}</p>
    <fieldset disabled={loading}>
      <legend>{t("settings.languageLegend")}</legend>
      <div className="form-grid">
        <label>{t("settings.interfaceLanguage")}
          <select value={language} onChange={event => setLanguage(event.currentTarget.value as Language)}>
            {LANGUAGES.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
          </select>
        </label>
        <label>{t("settings.feedbackLanguage")}
          <select value={preferences?.feedback_language ?? "auto"} disabled={!preferences} onChange={event => void saveFeedbackLanguage(event.currentTarget.value as Preferences["feedback_language"])}>
            <option value="auto">{t("settings.feedbackAuto")}</option>
            {LANGUAGES.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
          </select>
        </label>
      </div>
      <p className="supporting-text">{t("settings.languageHelp")}</p>
    </fieldset>
    <form onSubmit={event => void saveSettings(event)}>
      <fieldset disabled={loading}>
        <legend>{t("settings.inference")}</legend>
        <div className="form-grid">
          <label>{t("settings.url")}<input type="url" required maxLength={256} value={url} onChange={event => setUrl(event.currentTarget.value)} /></label>
          <label htmlFor="model-input">{t("settings.model")}<input id="model-input" required maxLength={128} value={model} onChange={event => setModel(event.currentTarget.value)} placeholder={t("settings.modelPlaceholder")} /></label>
        </div>
        <div className="action-row">
          <button type="submit" className="button-primary" disabled={!dirty || !url.trim() || !model.trim()}>{t("settings.save")}</button>
          <button type="button" onClick={() => void testConnection()} disabled={!url.trim() || !model.trim()}>{t("settings.test")}</button>
        </div>
        {dirty && saved && <p className="supporting-text">{t("settings.unsaved")}</p>}
      </fieldset>
    </form>
    {loading && <p role="status">{t("settings.loading")}</p>}
    {notice && <p className="notice" role="status">{notice}</p>}
    {response && <div><h3>{t("settings.response")}</h3><pre>{response}</pre></div>}
    {error && <pre role="alert">{error}</pre>}
    <BackupTools />
  </section>;
}
