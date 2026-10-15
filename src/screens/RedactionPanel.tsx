import { findManualSpans } from "../lib/redaction";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { IdentifierCandidate, Student, SubmissionSummary } from "../lib/types";
import { useI18n, type MessageKey } from "../lib/i18n";

const OTHER_STUDENT = "other";
const KIND_LABELS: Record<string, MessageKey> = {
  dni: "redaction.kind.dni",
  nie: "redaction.kind.nie",
  email: "redaction.kind.email",
  phone: "redaction.kind.phone",
  iban: "redaction.kind.iban",
  name_roster: "redaction.kind.roster",
};

function fold(value: string) {
  return value.normalize("NFD").replace(/\p{M}/gu, "").toLocaleLowerCase();
}

/**
 * Suggest the roster student whose name parts best match the file name and the
 * names detected in the text. Returns nothing when two students tie.
 */
export function suggestStudent(roster: Student[], hints: string[]): Student | undefined {
  const tokens = new Set(hints.flatMap(hint => fold(hint).split(/[^\p{L}]+/u)).filter(token => token.length >= 3));
  let best: Student | undefined;
  let bestScore = 0;
  let tie = false;
  for (const student of roster) {
    const parts = fold(student.full_name).split(/[^\p{L}]+/u).filter(part => part.length >= 3);
    const score = parts.filter(part => tokens.has(part)).length;
    if (score > bestScore) { best = student; bestScore = score; tie = false; }
    else if (score === bestScore && score > 0) tie = true;
  }
  return bestScore > 0 && !tie ? best : undefined;
}

/**
 * Mandatory human review of redaction. No text reaches the AI before explicit
 * confirmation; see `ConfirmedRedactedText` in the backend.
 */
export function RedactionPanel({
  submission,
  onConfirmed,
  blocked = false,
  roster = [],
  linkedStudentIds = [],
}: {
  submission: SubmissionSummary;
  onConfirmed: () => void;
  blocked?: boolean;
  roster?: Student[];
  linkedStudentIds?: number[];
}) {
  const { t, errorText } = useI18n();
  const [extraNames, setExtraNames] = useState("");
  const [candidates, setCandidates] = useState<IdentifierCandidate[]>([]);
  const [accepted, setAccepted] = useState<Set<number>>(new Set());
  const [manual, setManual] = useState("");
  const [choice, setChoice] = useState<string>(roster.length > 0 ? "" : OTHER_STUDENT);
  const [typedName, setTypedName] = useState("");
  const [suggested, setSuggested] = useState(false);
  const [result, setResult] = useState<{ alias: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ text: string; alias: string } | null>(null);
  const [reviewed, setReviewed] = useState(false);
  const pending = submission.status_pipeline === "ocr_complete";
  const selectedStudent = roster.find(student => String(student.id) === choice);
  const studentName = selectedStudent ? selectedStudent.full_name : choice === OTHER_STUDENT ? typedName : "";

  function invalidatePreview() { setPreview(null); setReviewed(false); }
  function rosterNames() {
    return [...roster.map(student => student.full_name), ...extraNames.split(/[\n,;]/).map(name => name.trim()).filter(Boolean)];
  }
  async function detect() {
    invalidatePreview();
    setError(null);
    setBusy(true);
    try {
      const list = await invoke<IdentifierCandidate[]>("cmd_detect_identifiers", { submissionId: submission.id, roster: rosterNames() });
      setCandidates(list);
      setAccepted(new Set(list.map((_, i) => i)));
      return list;
    } catch (cause) {
      setError(errorText(cause));
      return [];
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    setCandidates([]); setAccepted(new Set()); invalidatePreview();
    if (!pending || blocked) return;
    let active = true;
    void detect().then(list => {
      if (!active || roster.length === 0) return;
      const suggestion = suggestStudent(roster, [submission.source_name ?? "", ...list.filter(item => item.kind === "name_roster").map(item => item.text)]);
      if (suggestion) {
        setChoice(previous => previous === "" ? String(suggestion.id) : previous);
        setSuggested(true);
      }
    });
    return () => { active = false; };
  }, [submission.id, submission.text_ocr, blocked]);

  function selectedSpans(): [number, number][] {
    return [...candidates.filter((_, i) => accepted.has(i)).map((candidate): [number, number] => [candidate.start, candidate.end]), ...findManualSpans(submission.text_ocr ?? "", manual)];
  }
  async function previewRedaction() {
    setBusy(true); setError(null); invalidatePreview();
    try {
      const value = await invoke<{ text: string; alias: string }>("cmd_preview_redaction", { submissionId: submission.id, spans: selectedSpans(), studentName });
      setPreview(value);
    } catch (cause) { setError(errorText(cause)); } finally { setBusy(false); }
  }

  function toggle(index: number) {
    invalidatePreview();
    setAccepted(previous => {
      const copy = new Set(previous);
      if (copy.has(index)) copy.delete(index);
      else copy.add(index);
      return copy;
    });
  }

  async function confirm() {
    if (blocked) return;
    if (!preview || !reviewed) { setError(t("redaction.reviewFirst")); return; }
    if (!studentName.trim()) { setError(t("redaction.nameRequired")); return; }
    setError(null);
    setBusy(true);
    try {
      const alias = await invoke<string>("cmd_confirm_redaction", {
        submissionId: submission.id,
        spans: selectedSpans(),
        studentName,
        studentClassId: null,
        studentId: selectedStudent?.id ?? null,
        previewAlias: preview.alias,
        previewText: preview.text,
      });
      setResult({ alias });
      onConfirmed();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  }

  if (!pending) {
    return <p className="confirmed-assessment">{submission.identities_purged ? t("redaction.pseudonymized") : t("redaction.confirmed")}</p>;
  }

  return (
    <div className="review-panel">
      {blocked && <p className="warning-text">{t("redaction.blocked")}</p>}
      <h4>{t("redaction.title")}</h4>
      <p className="supporting-text">{t("redaction.intro")}</p>

      {roster.length > 0 ? <label>{t("redaction.student")}
        <select disabled={busy || blocked} value={choice} onChange={event => { setChoice(event.currentTarget.value); setSuggested(false); invalidatePreview(); }}>
          <option value="">{t("redaction.chooseStudent")}</option>
          {roster.map(student => <option key={student.id} value={student.id}>{student.full_name}{linkedStudentIds.includes(student.id) ? ` — ${t("redaction.alreadyLinked")}` : ""}</option>)}
          <option value={OTHER_STUDENT}>{t("redaction.otherStudent")}</option>
        </select>
      </label> : <p className="supporting-text">{t("redaction.noRoster")}</p>}
      {suggested && selectedStudent && <p className="supporting-text" role="status">{t("redaction.suggested")}</p>}
      {choice === OTHER_STUDENT && <label>
        {t("redaction.realName")}
        <input maxLength={200} disabled={busy || blocked} value={typedName} onChange={event => { setTypedName(event.currentTarget.value); invalidatePreview(); }} />
      </label>}

      <details>
        <summary>{t("redaction.moreNames")}</summary>
        <label>
          {t("redaction.extraNames")}
          <textarea disabled={busy || blocked} value={extraNames} onChange={event => { setExtraNames(event.currentTarget.value); invalidatePreview(); }} rows={2} />
        </label>
      </details>
      <div className="action-row compact"><button onClick={() => void detect()} disabled={busy || blocked}>{t("redaction.detect")}</button></div>

      {candidates.length > 0 ? (
        <>
          <h5>{t("redaction.candidates")}</h5>
          <ul className="candidate-list">
            {candidates.map((c, i) => (
              <li key={i}>
                <label>
                  <input type="checkbox" disabled={busy || blocked} checked={accepted.has(i)} onChange={() => toggle(i)} />{" "}
                  {t(KIND_LABELS[c.kind] ?? "redaction.kind.other")}: "{c.text}"
                </label>
              </li>
            ))}
          </ul>
        </>
      ) : !busy && <p className="supporting-text">{t("redaction.noCandidates")}</p>}

      <label>
        {t("redaction.manual")}
        <textarea disabled={busy || blocked} value={manual} onChange={event => { setManual(event.currentTarget.value); invalidatePreview(); }} rows={2} />
      </label>

      <p className="supporting-text">{t("redaction.checkHelp")}</p>
      <div className="action-row"><button onClick={() => void previewRedaction()} disabled={busy || blocked || !studentName.trim()}>{t("redaction.preview")}</button></div>
      {preview && <div className="redaction-preview">
        <h5>{t("redaction.previewTitle")}</h5>
        <pre className="document-preview" tabIndex={0} aria-label={t("redaction.previewLabel")}>{preview.text}</pre>
        <label><input type="checkbox" checked={reviewed} disabled={busy || blocked} onChange={event => setReviewed(event.currentTarget.checked)} />{t("redaction.reviewed")}</label>
      </div>}
      <div className="action-row"><button className="button-primary" onClick={() => void confirm()} disabled={busy || blocked || !preview || !reviewed}>{t("redaction.confirm")}</button></div>

      {error && <pre role="alert">{error}</pre>}
      {result && <p className="notice" role="status">{t("redaction.done", { alias: result.alias })}</p>}
    </div>
  );
}
