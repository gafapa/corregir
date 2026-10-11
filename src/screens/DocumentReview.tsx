import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { SubmissionSummary } from "../lib/types";

type PagePreview = { data_url: string; page: number; total: number };

export function DocumentReview({ submission, onSaved, onDraftChange }: { submission: SubmissionSummary; onSaved: () => Promise<void>; onDraftChange: (dirty: boolean) => void }) {
  const [text, setText] = useState(submission.text_ocr ?? "");
  const [filePath, setFilePath] = useState<string | null>(null);
  const [preview, setPreview] = useState<PagePreview | null>(null);
  const [zoom, setZoom] = useState(100);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const active = useRef(true);
  useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  useEffect(() => { setText(submission.text_ocr ?? ""); onDraftChange(false); }, [submission.text_ocr]);
  const dirty = text !== (submission.text_ocr ?? "");
  const editable = submission.status_pipeline === "ocr_complete";

  async function loadPage(path: string, page: number) {
    setBusy(true); setError(null);
    try {
      const result = await invoke<PagePreview>("cmd_preview_document", { submissionId: submission.id, filePath: path, page });
      if (active.current) { setPreview(result); setFilePath(path); }
    } catch (error) { if (active.current) { setError(String(error)); setPreview(null); } }
    finally { if (active.current) setBusy(false); }
  }
  async function selectOriginal() {
    setError(null);
    try {
      const path = await open({ multiple: false, filters: [{ name: "Original document", extensions: ["pdf", "png", "jpg", "jpeg"] }] });
      if (typeof path === "string") await loadPage(path, 1);
    } catch (error) { setError(String(error)); }
  }
  async function saveText() {
    setBusy(true); setError(null); setNotice("");
    try {
      await invoke("cmd_save_extracted_text", { submissionId: submission.id, expectedText: submission.text_ocr ?? "", text });
      await onSaved();
      if (active.current) setNotice("Corrected text saved. Review identifiers using the updated text.");
    } catch (error) { if (active.current) setError(String(error)); }
    finally { if (active.current) setBusy(false); }
  }
  return <details className="document-review">
    <summary>Compare original and extracted text</summary>
    <p className="supporting-text">Select the original PDF or image to display its pages locally. Its fingerprint must match the imported document. The app does not store another copy.</p>
    <button onClick={selectOriginal} disabled={busy}>Select original document</button>
    <div className="document-comparison">
      <div className="original-page">
        <h5>Original page</h5>
        {preview ? <>
          <div className="action-row">
            <button disabled={busy || preview.page <= 1} onClick={() => { if (filePath) void loadPage(filePath, preview.page - 1); }}>Previous page</button>
            <span>Page {preview.page} of {preview.total}</span>
            <button disabled={busy || preview.page >= preview.total} onClick={() => { if (filePath) void loadPage(filePath, preview.page + 1); }}>Next page</button>
          </div>
          <label>Original page zoom<select value={zoom} disabled={busy} onChange={event => setZoom(Number(event.currentTarget.value))}><option value={100}>100%</option><option value={150}>150%</option><option value={200}>200%</option><option value={300}>300%</option></select></label>
          <div className="page-viewport" tabIndex={0} role="region" aria-label="Original page preview; scroll to inspect the enlarged page"><img style={{ width: `${zoom}%` }} src={preview.data_url} alt={`Original document, page ${preview.page} of ${preview.total}`} /></div>
        </> : <p className="supporting-text">No original selected. For Word documents, compare the original in your document editor.</p>}
      </div>
      <div>
        <label htmlFor={`extracted-text-${submission.id}`}>Extracted text (all pages)</label>
        <textarea id={`extracted-text-${submission.id}`} value={text} readOnly={!editable} disabled={busy} maxLength={1048576} rows={16} onChange={event => { setText(event.currentTarget.value); setNotice(""); onDraftChange(event.currentTarget.value !== (submission.text_ocr ?? "")); }} />
        {editable ? <>
          <p className="supporting-text">Correct recognition mistakes only; preserve the student's answer. Save corrections before reviewing redaction.</p>
          <div className="action-row"><button className="button-primary" onClick={saveText} disabled={busy || !dirty || !text.trim()}>Save corrected text</button><button disabled={busy || !dirty} onClick={() => { setText(submission.text_ocr ?? ""); onDraftChange(false); }}>Discard text changes</button></div>
        </> : <p className="supporting-text">Text is locked after redaction confirmation to preserve the reviewed assessment.</p>}
      </div>
    </div>
    {busy && <p role="status">Loading or saving document review...</p>}
    {dirty && <p className="warning-text" role="status">Unsaved text corrections. Save or discard them before redaction.</p>}
    {notice && <p className="notice" role="status">{notice}</p>}
    {error && <pre role="alert">{error}</pre>}
  </details>;
}
