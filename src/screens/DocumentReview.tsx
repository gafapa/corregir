import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { SubmissionSummary } from "../lib/types";
import { useI18n } from "../lib/i18n";

type PagePreview = { data_url: string; page: number; total: number };

export function DocumentReview({ submission, onSaved, onDraftChange }: { submission: SubmissionSummary; onSaved: () => Promise<void>; onDraftChange: (dirty: boolean) => void }) {
  const { t, errorText } = useI18n();
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
    } catch (cause) { if (active.current) { setError(errorText(cause)); setPreview(null); } }
    finally { if (active.current) setBusy(false); }
  }
  async function selectOriginal() {
    setError(null);
    try {
      const path = await open({ multiple: false, filters: [{ name: t("review.originalDocument"), extensions: ["pdf", "png", "jpg", "jpeg"] }] });
      if (typeof path === "string") await loadPage(path, 1);
    } catch (cause) { setError(errorText(cause)); }
  }
  async function saveText() {
    setBusy(true); setError(null); setNotice("");
    try {
      await invoke("cmd_save_extracted_text", { submissionId: submission.id, expectedText: submission.text_ocr ?? "", text });
      await onSaved();
      if (active.current) setNotice(t("review.saved"));
    } catch (cause) { if (active.current) setError(errorText(cause)); }
    finally { if (active.current) setBusy(false); }
  }
  return <details className="document-review">
    <summary>{t("review.summary")}</summary>
    <p className="supporting-text">{t("review.help")}{submission.source_name ? ` ${t("review.importedFile", { name: submission.source_name })}` : ""}</p>
    <button onClick={() => void selectOriginal()} disabled={busy}>{t("review.selectOriginal")}</button>
    <div className="document-comparison">
      <div className="original-page">
        <h5>{t("review.originalPage")}</h5>
        {preview ? <>
          <div className="action-row">
            <button disabled={busy || preview.page <= 1} onClick={() => { if (filePath) void loadPage(filePath, preview.page - 1); }}>{t("review.previousPage")}</button>
            <span>{t("review.pageOf", { page: preview.page, total: preview.total })}</span>
            <button disabled={busy || preview.page >= preview.total} onClick={() => { if (filePath) void loadPage(filePath, preview.page + 1); }}>{t("review.nextPage")}</button>
          </div>
          <label>{t("review.zoom")}<select value={zoom} disabled={busy} onChange={event => setZoom(Number(event.currentTarget.value))}><option value={100}>100%</option><option value={150}>150%</option><option value={200}>200%</option><option value={300}>300%</option></select></label>
          <div className="page-viewport" tabIndex={0} role="region" aria-label={t("review.viewport")}><img style={{ width: `${zoom}%` }} src={preview.data_url} alt={t("review.pageAlt", { page: preview.page, total: preview.total })} /></div>
        </> : <p className="supporting-text">{t("review.noOriginal")}</p>}
      </div>
      <div>
        <label htmlFor={`extracted-text-${submission.id}`}>{t("review.extracted")}</label>
        <textarea id={`extracted-text-${submission.id}`} value={text} readOnly={!editable} disabled={busy} maxLength={1048576} rows={16} onChange={event => { setText(event.currentTarget.value); setNotice(""); onDraftChange(event.currentTarget.value !== (submission.text_ocr ?? "")); }} />
        {editable ? <>
          <p className="supporting-text">{t("review.correctOnly")}</p>
          <div className="action-row"><button className="button-primary" onClick={() => void saveText()} disabled={busy || !dirty || !text.trim()}>{t("review.save")}</button><button disabled={busy || !dirty} onClick={() => { setText(submission.text_ocr ?? ""); onDraftChange(false); }}>{t("review.discard")}</button></div>
        </> : <p className="supporting-text">{t("review.locked")}</p>}
      </div>
    </div>
    {busy && <p role="status">{t("review.busy")}</p>}
    {dirty && <p className="warning-text" role="status">{t("review.unsaved")}</p>}
    {notice && <p className="notice" role="status">{notice}</p>}
    {error && <pre role="alert">{error}</pre>}
  </details>;
}
