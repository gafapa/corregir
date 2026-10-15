import { useEffect, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { SYNTHETIC_RUBRICS, type NewRubric, type RubricWithCriteria } from "../lib/types";
import { useI18n } from "../lib/i18n";

type EditableLevel = { key: number; label: string; score: string; description: string };
type EditableCriterion = { key: number; code: string; description: string; score: string; levels: EditableLevel[] };

export function Rubrics() {
  const { t, formatNumber, errorText } = useI18n();
  const [rubrics, setRubrics] = useState<RubricWithCriteria[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState<{ id: number; version: number } | null>(null);
  const [title, setTitle] = useState("");
  const [subject, setSubject] = useState("");
  const [gradeLevel, setGradeLevel] = useState("");
  const [criteria, setCriteria] = useState<EditableCriterion[]>([{ key: 0, code: "C1", description: "", score: "10", levels: [] }]);
  const nextKey = useRef(1);
  const titleInput = useRef<HTMLInputElement>(null);

  async function reload() {
    const list = await invoke<RubricWithCriteria[]>("cmd_list_rubrics");
    setRubrics(list);
  }
  useEffect(() => {
    let active = true;
    invoke<RubricWithCriteria[]>("cmd_list_rubrics").then(list => { if (active) setRubrics(list); })
      .catch(cause => { if (active) setError(errorText(cause)); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, []);

  function resetEditor() {
    setEditing(null); setTitle(""); setSubject(""); setGradeLevel("");
    setCriteria([{ key: nextKey.current++, code: "C1", description: "", score: "10", levels: [] }]);
  }
  function edit(rubric: RubricWithCriteria) {
    setEditing({ id: rubric.id, version: rubric.version });
    setTitle(rubric.title); setSubject(rubric.subject); setGradeLevel(rubric.grade_level);
    setCriteria(rubric.criteria.map(item => ({
      key: nextKey.current++, code: item.code, description: item.description, score: String(item.score_max),
      levels: (item.levels ?? []).map(level => ({ key: nextKey.current++, label: level.label, score: String(level.score), description: level.description })),
    })));
    setError(null); setNotice(""); titleInput.current?.focus();
  }
  function updateCriterion(key: number, field: "code" | "description" | "score", value: string) {
    setCriteria(previous => previous.map(item => item.key === key ? { ...item, [field]: value } : item));
  }
  function updateLevels(key: number, update: (levels: EditableLevel[]) => EditableLevel[]) {
    setCriteria(previous => previous.map(item => item.key === key ? { ...item, levels: update(item.levels) } : item));
  }
  function addCriterion() {
    let index = criteria.length + 1;
    while (criteria.some(item => item.code === `C${index}`)) index++;
    setCriteria(previous => [...previous, { key: nextKey.current++, code: `C${index}`, description: "", score: "1", levels: [] }]);
  }
  function suggestLevels(criterion: EditableCriterion) {
    const maximum = Number(criterion.score) || 0;
    updateLevels(criterion.key, () => [
      { key: nextKey.current++, label: t("rubrics.level.excellent"), score: String(maximum), description: "" },
      { key: nextKey.current++, label: t("rubrics.level.good"), score: String(Math.round(maximum * 0.75 * 100) / 100), description: "" },
      { key: nextKey.current++, label: t("rubrics.level.partial"), score: String(Math.round(maximum * 0.5 * 100) / 100), description: "" },
      { key: nextKey.current++, label: t("rubrics.level.notYet"), score: "0", description: "" },
    ]);
  }
  async function saveRubric(event: FormEvent) {
    event.preventDefault(); setError(null); setNotice("");
    const rubric: NewRubric = {
      title: title.trim(), subject: subject.trim(), grade_level: gradeLevel.trim(),
      criteria: criteria.map(item => ({
        code: item.code.trim(), description: item.description.trim(), score_max: Number(item.score),
        levels: item.levels.filter(level => level.label.trim()).map(level => ({ label: level.label.trim(), score: Number(level.score), description: level.description.trim() })),
      })),
    };
    if (new Set(rubric.criteria.map(item => item.code)).size !== criteria.length) { setError(t("rubrics.uniqueCodes")); return; }
    if (rubric.criteria.some(item => !item.description || !Number.isFinite(item.score_max) || item.score_max <= 0 || item.score_max > 10000)) { setError(t("rubrics.invalidCriterion")); return; }
    if (rubric.criteria.some(item => (item.levels ?? []).some(level => !Number.isFinite(level.score) || level.score < 0 || level.score > item.score_max))) { setError(t("rubrics.invalidLevel")); return; }
    setLoading(true);
    try {
      const id = editing
        ? await invoke<number>("cmd_create_rubric_version", { rubricId: editing.id, expectedVersion: editing.version, rubric })
        : await invoke<number>("cmd_create_rubric", { rubric });
      resetEditor(); await reload();
      setNotice(t("rubrics.saved", { id }));
    } catch (cause) { setError(errorText(cause)); } finally { setLoading(false); }
  }
  async function createSyntheticRubric(index: number) {
    setLoading(true); setError(null); setNotice("");
    try { const id = await invoke<number>("cmd_create_rubric", { rubric: SYNTHETIC_RUBRICS[index] }); await reload(); setNotice(t("rubrics.syntheticSaved", { id })); }
    catch (cause) { setError(errorText(cause)); } finally { setLoading(false); }
  }
  async function deleteRubric(rubric: RubricWithCriteria) {
    const confirmed = await ask(t("rubrics.deleteConfirm", { name: rubric.title, version: rubric.version }), { title: t("rubrics.delete"), kind: "warning", okLabel: t("common.delete"), cancelLabel: t("common.cancel") });
    if (!confirmed) return;
    setLoading(true); setError(null); setNotice("");
    try { await invoke("cmd_delete_rubric", { rubricId: rubric.id }); await reload(); setNotice(t("rubrics.deleted")); }
    catch (cause) { setError(errorText(cause)); } finally { setLoading(false); }
  }
  const total = criteria.reduce((sum, item) => sum + (Number(item.score) || 0), 0);
  return (
    <section aria-labelledby="rubrics-title">
      <h2 id="rubrics-title">{t("rubrics.title")}</h2>
      <p className="page-description">{t("rubrics.description")}</p>
      <form onSubmit={event => void saveRubric(event)}>
        <fieldset disabled={loading} className="rubric-editor">
          <legend>{editing ? t("rubrics.editLegend", { id: editing.id, version: editing.version + 1 }) : t("rubrics.newLegend")}</legend>
          <label>{t("rubrics.titleField")}<input ref={titleInput} required maxLength={200} value={title} onChange={event => setTitle(event.currentTarget.value)} /></label>
          <div className="form-grid">
            <label>{t("rubrics.subject")}<input required maxLength={200} value={subject} onChange={event => setSubject(event.currentTarget.value)} /></label>
            <label>{t("rubrics.gradeLevel")}<input required maxLength={100} value={gradeLevel} onChange={event => setGradeLevel(event.currentTarget.value)} /></label>
          </div>
          <h3>{t("rubrics.criteriaTotal", { points: Number.isFinite(total) ? formatNumber(total) : "—" })}</h3>
          {criteria.map((item, index) => <div className="criterion-editor" key={item.key}>
            <div className="criterion-editor-fields">
              <label>{t("rubrics.code", { index: index + 1 })}<input required maxLength={32} pattern={"[A-Za-z0-9_\\-]+"} value={item.code} onChange={event => updateCriterion(item.key, "code", event.currentTarget.value)} /></label>
              <label>{t("rubrics.criterionDescription", { index: index + 1 })}<textarea required maxLength={4000} value={item.description} onChange={event => updateCriterion(item.key, "description", event.currentTarget.value)} /></label>
              <label>{t("rubrics.maximum", { index: index + 1 })}<input type="number" required min={0.01} max={10000} step="any" value={item.score} onChange={event => updateCriterion(item.key, "score", event.currentTarget.value)} /></label>
            </div>
            <details className="levels-editor" open={item.levels.length > 0 ? true : undefined}>
              <summary>{t("rubrics.levels", { count: item.levels.length })}</summary>
              <p className="supporting-text">{t("rubrics.levelsHelp")}</p>
              {item.levels.map((level, levelIndex) => <div className="level-row" key={level.key}>
                <label>{t("rubrics.levelName", { index: levelIndex + 1 })}<input maxLength={100} value={level.label} onChange={event => { const value = event.currentTarget.value; updateLevels(item.key, levels => levels.map(entry => entry.key === level.key ? { ...entry, label: value } : entry)); }} /></label>
                <label>{t("rubrics.levelScore", { index: levelIndex + 1 })}<input type="number" min={0} max={Number(item.score) || undefined} step="any" value={level.score} onChange={event => { const value = event.currentTarget.value; updateLevels(item.key, levels => levels.map(entry => entry.key === level.key ? { ...entry, score: value } : entry)); }} /></label>
                <label>{t("rubrics.levelDescription", { index: levelIndex + 1 })}<input maxLength={1000} value={level.description} onChange={event => { const value = event.currentTarget.value; updateLevels(item.key, levels => levels.map(entry => entry.key === level.key ? { ...entry, description: value } : entry)); }} /></label>
                <button type="button" className="button-quiet" onClick={() => updateLevels(item.key, levels => levels.filter(entry => entry.key !== level.key))}>{t("rubrics.removeLevel", { index: levelIndex + 1 })}</button>
              </div>)}
              <div className="action-row compact">
                <button type="button" disabled={item.levels.length >= 10} onClick={() => updateLevels(item.key, levels => [...levels, { key: nextKey.current++, label: "", score: "", description: "" }])}>{t("rubrics.addLevel")}</button>
                {item.levels.length === 0 && <button type="button" className="button-quiet" onClick={() => suggestLevels(item)}>{t("rubrics.suggestLevels")}</button>}
              </div>
            </details>
            <button type="button" className="button-quiet" disabled={criteria.length === 1} onClick={() => setCriteria(previous => previous.filter(criterion => criterion.key !== item.key))}>{t("rubrics.removeCriterion", { index: index + 1 })}</button>
          </div>)}
          <div className="action-row">
            <button type="button" disabled={criteria.length >= 100} onClick={addCriterion}>{t("rubrics.addCriterion")}</button>
            <button className="button-primary" type="submit">{editing ? t("rubrics.saveVersion") : t("rubrics.save")}</button>
            {editing && <button type="button" onClick={resetEditor}>{t("rubrics.cancelEdit")}</button>}
          </div>
        </fieldset>
      </form>
      {loading && <p role="status">{t("common.loading")}</p>}
      {notice && <p className="notice" role="status">{notice}</p>}
      {error && <pre role="alert">{error}</pre>}
      <details className="synthetic-rubric-tools"><summary>{t("rubrics.synthetic")}</summary>
        <div className="action-row">{SYNTHETIC_RUBRICS.map((rubric, index) => <button key={rubric.title} disabled={loading} onClick={() => void createSyntheticRubric(index)}>{t("rubrics.createSynthetic", { name: rubric.title })}</button>)}</div>
      </details>
      <h3>{t("rubrics.savedTitle", { count: rubrics.length })}</h3>
      {!loading && rubrics.length === 0 && <div className="empty-state"><strong>{t("rubrics.emptyTitle")}</strong><p>{t("rubrics.emptyText")}</p></div>}
      <ul className="rubric-list">{rubrics.map(rubric => <li key={rubric.id}>
        <div className="submission-heading"><div><strong>{rubric.title}</strong><p className="supporting-text">{rubric.subject} · {rubric.grade_level} · {t("rubrics.version", { version: rubric.version })}</p></div>
          <div className="action-row compact">
            <button disabled={loading} onClick={() => edit(rubric)} aria-label={t("rubrics.editNamed", { name: rubric.title, version: rubric.version })}>{t("rubrics.editAsNew")}</button>
            <button className="button-quiet button-danger" disabled={loading} onClick={() => void deleteRubric(rubric)} aria-label={t("rubrics.deleteNamed", { name: rubric.title, version: rubric.version })}>{t("common.delete")}</button>
          </div>
        </div>
        <ul>{rubric.criteria.map(criterion => <li key={criterion.id}>{criterion.code}: {criterion.description} ({t("grading.points", { points: formatNumber(criterion.score_max) })}){(criterion.levels ?? []).length > 0 ? ` · ${(criterion.levels ?? []).map(level => `${level.label} ${formatNumber(level.score)}`).join(" / ")}` : ""}</li>)}</ul>
      </li>)}</ul>
    </section>
  );
}
