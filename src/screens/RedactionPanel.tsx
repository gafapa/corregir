import { findManualSpans } from "../lib/redaction";
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { IdentifierCandidate, SubmissionSummary } from "../lib/types";

/**
 * Milestone 4: mandatory human review of redaction. No text reaches the AI before explicit confirmation; see `ConfirmedRedactedText` in the backend.
 */
export function RedactionPanel({
  submission,
  onConfirmed,
}: {
  submission: SubmissionSummary;
  onConfirmed: () => void;
}) {
  const [roster, setRoster] = useState("");
  const [candidates, setCandidates] = useState<IdentifierCandidate[]>([]);
  const [accepted, setAccepted] = useState<Set<number>>(new Set());
  const [manual, setManual] = useState("");
  const [studentName, setStudentName] = useState("");
  const [result, setResult] = useState<{ alias: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function detect() {
    setError(null);
    setBusy(true);
    try {
      const names = roster
        .split(/[\n,]/)
        .map((n) => n.trim())
        .filter(Boolean);
      const list = await invoke<IdentifierCandidate[]>("cmd_detect_identifiers", {
        submissionId: submission.id,
        roster: names,
      });
      setCandidates(list);
      setAccepted(new Set(list.map((_, i) => i)));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function toggle(index: number) {
    setAccepted((prev) => {
      const copy = new Set(prev);
      if (copy.has(index)) copy.delete(index);
      else copy.add(index);
      return copy;
    });
  }

  async function confirm() {
    if (!studentName.trim()) {
      setError("Enter the student's real name. It stays local until you export a grading report.");
      return;
    }
    setError(null);
    setBusy(true);
    try {
      const spans: [number, number][] = [
        ...candidates.filter((_, i) => accepted.has(i)).map((c): [number, number] => [c.start, c.end]),
        ...findManualSpans(submission.text_ocr ?? "", manual),
      ];
      const alias = await invoke<string>("cmd_confirm_redaction", {
        submissionId: submission.id,
        spans,
        studentName,
        studentClassId: null,
      });
      setResult({ alias });
      onConfirmed();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  if (["redacted", "grade_confirmed"].includes(submission.status_pipeline)) {
    return <p>✅ Redaction confirmed.</p>;
  }

  return (
    <div style={{ border: "1px solid #ccc", padding: "0.5rem", marginTop: "0.5rem" }}>
      <h4>Redact submission #{submission.id}</h4>

      <label>
        Class roster (one full name per line or comma separated):
        <textarea
          value={roster}
          onChange={(e) => setRoster(e.currentTarget.value)}
          rows={2}
          style={{ width: "100%" }}
        />
      </label>
      <button onClick={detect} disabled={busy}>Detect identifiers</button>

      {candidates.length > 0 && (
        <>
          <h5>Detected candidates (review before confirming):</h5>
          <ul>
            {candidates.map((c, i) => (
              <li key={i}>
                <label>
                  <input
                    type="checkbox"
                    checked={accepted.has(i)}
                    onChange={() => toggle(i)}
                  />{" "}
                  [{c.kind}] "{c.text}"
                </label>
              </li>
            ))}
          </ul>
        </>
      )}

      <label>
        Additional text to redact (one exact fragment per line):
        <textarea
          value={manual}
          onChange={(e) => setManual(e.currentTarget.value)}
          rows={2}
          style={{ width: "100%" }}
        />
      </label>

      <label>
        Student's real name (stored locally, never sent to the AI):
        <input value={studentName} onChange={(e) => setStudentName(e.currentTarget.value)} />
      </label>

      <button onClick={confirm} disabled={busy}>Confirm redaction</button>

      {error && <pre role="alert" style={{ color: "crimson" }}>{error}</pre>}
      {result && (
        <p>
          ✅ Confirmed. Assigned alias: <code>{result.alias}</code>
        </p>
      )}
    </div>
  );
}
