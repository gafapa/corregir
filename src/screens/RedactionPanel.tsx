import { findManualSpans } from "../lib/redaction";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { IdentifierCandidate, SubmissionSummary } from "../lib/types";

/**
 * Milestone 4: mandatory human review of redaction. No text reaches the AI before explicit confirmation; see `ConfirmedRedactedText` in the backend.
 */
export function RedactionPanel({
  submission,
  onConfirmed,
  blocked = false,
}: {
  submission: SubmissionSummary;
  onConfirmed: () => void;
  blocked?: boolean;
}) {
  const [roster, setRoster] = useState("");
  const [candidates, setCandidates] = useState<IdentifierCandidate[]>([]);
  const [accepted, setAccepted] = useState<Set<number>>(new Set());
  const [manual, setManual] = useState("");
  const [studentName, setStudentName] = useState("");
  const [result, setResult] = useState<{ alias: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ text: string; alias: string } | null>(null);
  const [reviewed, setReviewed] = useState(false);
  useEffect(() => { setCandidates([]); setAccepted(new Set()); setPreview(null); setReviewed(false); }, [submission.text_ocr, blocked]);
  function invalidatePreview() { setPreview(null); setReviewed(false); }
  function selectedSpans(): [number, number][] {
    return [...candidates.filter((_, i) => accepted.has(i)).map((candidate): [number, number] => [candidate.start, candidate.end]), ...findManualSpans(submission.text_ocr ?? "", manual)];
  }
  async function previewRedaction() {
    setBusy(true); setError(null); invalidatePreview();
    try {
      const result = await invoke<{ text: string; alias: string }>("cmd_preview_redaction", { submissionId: submission.id, spans: selectedSpans(), studentName });
      setPreview(result);
    } catch (error) { setError(String(error)); } finally { setBusy(false); }
  }

  async function detect() {
    invalidatePreview();
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
    invalidatePreview();
    setAccepted((prev) => {
      const copy = new Set(prev);
      if (copy.has(index)) copy.delete(index);
      else copy.add(index);
      return copy;
    });
  }

  async function confirm() {
    if (blocked) return;
    if (!preview || !reviewed) { setError("Generate and review the redacted text before confirming."); return; }
    if (!studentName.trim()) {
      setError("Enter the student's real name. It stays local until you export a grading report.");
      return;
    }
    setError(null);
    setBusy(true);
    try {
      const spans = selectedSpans();
      const alias = await invoke<string>("cmd_confirm_redaction", {
        submissionId: submission.id,
        spans,
        studentName,
        studentClassId: null,
        previewAlias: preview.alias,
        previewText: preview.text,
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
    return <p className="confirmed-assessment">Redaction confirmed.</p>;
  }

  return (
    <div className="review-panel">
      {blocked && <p className="warning-text">Save or discard text corrections before reviewing redaction.</p>}
      <h4>Redact submission #{submission.id}</h4>

      <label>
        Class roster (one full name per line or comma separated):
        <textarea
          disabled={busy || blocked}
          value={roster}
          onChange={(e) => { setRoster(e.currentTarget.value); invalidatePreview(); }}
          rows={2}
        />
      </label>
      <button onClick={detect} disabled={busy || blocked}>Detect identifiers</button>

      {candidates.length > 0 && (
        <>
          <h5>Detected candidates (review before confirming):</h5>
          <ul>
            {candidates.map((c, i) => (
              <li key={i}>
                <label>
                  <input
                    type="checkbox"
                    disabled={busy || blocked}
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
          disabled={busy || blocked}
          value={manual}
          onChange={(e) => { setManual(e.currentTarget.value); invalidatePreview(); }}
          rows={2}
        />
      </label>

      <label>
        Student's real name (stored locally; full-name matches are redacted automatically):
        <input maxLength={200} disabled={busy || blocked} value={studentName} onChange={(e) => { setStudentName(e.currentTarget.value); invalidatePreview(); }} />
      </label>

      <p className="supporting-text">Review OCR against the original document first. Check name variants, OCR mistakes and other identifiers in the preview; automatic detection cannot find every identifier.</p>
      <div className="action-row"><button onClick={previewRedaction} disabled={busy || blocked || !studentName.trim()}>Preview redacted text</button></div>
      {preview && <div className="redaction-preview">
        <h5>Submission text that Ollama will receive</h5>
        <pre className="document-preview" tabIndex={0} aria-label="Redacted submission preview">{preview.text}</pre>
        <label><input type="checkbox" checked={reviewed} disabled={busy || blocked} onChange={event => setReviewed(event.currentTarget.checked)} />I checked this text for remaining identifiers.</label>
      </div>}
      <div className="action-row"><button className="button-primary" onClick={confirm} disabled={busy || blocked || !preview || !reviewed}>Confirm redaction</button></div>

      {error && <pre role="alert">{error}</pre>}
      {result && (
        <p className="notice" role="status">
          Confirmed. Assigned alias: <code>{result.alias}</code>
        </p>
      )}
    </div>
  );
}
