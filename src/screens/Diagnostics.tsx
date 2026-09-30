import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Milestone 1: temporary diagnostic screen. Tests the full round trip from
 * React through Tauri (Rust) to Ollama. No student data is sent here.
 */
export function Diagnostics() {
  const [model, setModel] = useState("qwen3:8b");
  const [response, setResponse] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function testConnection() {
    setLoading(true);
    setError(null);
    setResponse(null);
    try {
      const result = await invoke<string>("cmd_test_ai_connection", {
        model,
      });
      setResponse(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <section>
      <h2>Diagnostics (Milestone 1)</h2>
      <p>Test the connection to the local inference server (Ollama).</p>

      <div className="row">
        <label htmlFor="model-input">Model:</label>
        <input
          id="model-input"
          value={model}
          onChange={(e) => setModel(e.currentTarget.value)}
          disabled={loading}
          placeholder="Model name in Ollama"
        />
        <button onClick={testConnection} disabled={loading}>
          {loading ? "Testing…" : "Test AI connection"}
        </button>
      </div>

      {response && (
        <div>
          <h3>Model response:</h3>
          <pre>{response}</pre>
        </div>
      )}

      {error && (
        <div>
          <h3>Error:</h3>
          <pre role="alert" style={{ color: "crimson" }}>{error}</pre>
        </div>
      )}
    </section>
  );
}
