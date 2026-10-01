import { useEffect, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BackupTools } from "./BackupTools";
type OllamaSettings = { url: string; model: string };

export function Diagnostics() {
  const [model, setModel] = useState("qwen3:8b");
  const [url, setUrl] = useState("http://127.0.0.1:11434");
  const [saved, setSaved] = useState<OllamaSettings | null>(null);
  const [response, setResponse] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let active = true;
    invoke<OllamaSettings>("cmd_load_ollama_settings").then(settings => {
      if (active) { setUrl(settings.url); setModel(settings.model); setSaved(settings); }
    }).catch(error => { if (active) setError(String(error)); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, []);
  async function saveSettings(event: FormEvent) {
    event.preventDefault(); setLoading(true); setError(null); setNotice("");
    const settings = { url: url.trim(), model: model.trim() };
    try { await invoke("cmd_save_ollama_settings", { settings }); setUrl(settings.url); setModel(settings.model); setSaved(settings); setNotice("Saved. Evidence and feedback requests will use this local server and model."); }
    catch (error) { setError(String(error)); } finally { setLoading(false); }
  }
  async function testConnection() {
    setLoading(true); setError(null); setResponse(null); setNotice("");
    try { setResponse(await invoke<string>("cmd_test_ai_connection", { model: model.trim(), url: url.trim() })); }
    catch (error) { setError(String(error)); } finally { setLoading(false); }
  }
  const dirty = saved?.url !== url.trim() || saved?.model !== model.trim();
  return <section aria-labelledby="diagnostics-title">
    <h2 id="diagnostics-title">Settings and diagnostics</h2>
    <p className="page-description">Choose the local Ollama server and model used for evidence and feedback. Remote servers are not supported.</p>
    <form onSubmit={saveSettings}>
      <fieldset disabled={loading}>
        <legend>Local inference</legend>
        <div className="form-grid">
          <label>Server URL<input type="url" required maxLength={256} value={url} onChange={event => setUrl(event.currentTarget.value)} /></label>
          <label htmlFor="model-input">Model:<input id="model-input" required maxLength={128} value={model} onChange={event => setModel(event.currentTarget.value)} placeholder="Model name in Ollama" /></label>
        </div>
        <div className="action-row">
          <button type="submit" className="button-primary" disabled={!dirty || !url.trim() || !model.trim()}>Save inference settings</button>
          <button type="button" onClick={testConnection} disabled={!url.trim() || !model.trim()}>Test AI connection</button>
        </div>
        {dirty && saved && <p className="supporting-text">Unsaved changes. Testing uses the values above; correction requests use the saved configuration.</p>}
      </fieldset>
    </form>
    {loading && <p role="status">Loading, saving or testing local inference…</p>}
    {notice && <p className="notice" role="status">{notice}</p>}
    {response && <div><h3>Model response</h3><pre>{response}</pre></div>}
    {error && <pre role="alert">{error}</pre>}
    <BackupTools />
  </section>;
}
