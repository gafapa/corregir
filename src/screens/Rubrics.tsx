import { useEffect, useRef, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SYNTHETIC_RUBRICS, type NewRubric, type RubricWithCriteria } from "../lib/types";

type EditableCriterion = { key: number; code: string; description: string; score: string };
export function Rubrics() {
  const [rubrics, setRubrics] = useState<RubricWithCriteria[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState<{ id: number; version: number } | null>(null);
  const [title, setTitle] = useState("");
  const [subject, setSubject] = useState("");
  const [gradeLevel, setGradeLevel] = useState("");
  const [criteria, setCriteria] = useState<EditableCriterion[]>([{ key: 0, code: "C1", description: "", score: "10" }]);
  const nextKey = useRef(1);
  const titleInput = useRef<HTMLInputElement>(null);

  async function reload() {
    const list = await invoke<RubricWithCriteria[]>("cmd_list_rubrics");
    setRubrics(list);
  }
  useEffect(() => {
    let active = true;
    invoke<RubricWithCriteria[]>("cmd_list_rubrics").then(list => { if (active) setRubrics(list); })
      .catch(error => { if (active) setError(String(error)); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, []);

  function resetEditor() {
    setEditing(null); setTitle(""); setSubject(""); setGradeLevel("");
    setCriteria([{ key: nextKey.current++, code: "C1", description: "", score: "10" }]);
  }
  function edit(rubric: RubricWithCriteria) {
    setEditing({ id: rubric.id, version: rubric.version });
    setTitle(rubric.title); setSubject(rubric.subject); setGradeLevel(rubric.grade_level);
    setCriteria(rubric.criteria.map(item => ({ key: nextKey.current++, code: item.code, description: item.description, score: String(item.score_max) })));
    setError(null); setNotice(""); titleInput.current?.focus();
  }
  function updateCriterion(key: number, field: "code" | "description" | "score", value: string) {
    setCriteria(previous => previous.map(item => item.key === key ? { ...item, [field]: value } : item));
  }
  function addCriterion() {
    let index = criteria.length + 1;
    while (criteria.some(item => item.code === `C${index}`)) index++;
    setCriteria(previous => [...previous, { key: nextKey.current++, code: `C${index}`, description: "", score: "1" }]);
  }
  async function saveRubric(event: FormEvent) {
    event.preventDefault(); setError(null); setNotice("");
    const rubric: NewRubric = { title: title.trim(), subject: subject.trim(), grade_level: gradeLevel.trim(), criteria: criteria.map(item => ({ code: item.code.trim(), description: item.description.trim(), score_max: Number(item.score) })) };
    if (new Set(rubric.criteria.map(item => item.code)).size !== criteria.length) { setError("Each criterion needs a unique code."); return; }
    if (rubric.criteria.some(item => !item.description || !Number.isFinite(item.score_max) || item.score_max <= 0 || item.score_max > 10000)) { setError("Each criterion needs a description and a maximum score between 0 and 10000, excluding zero."); return; }
    setLoading(true);
    try {
      const id = editing
        ? await invoke<number>("cmd_create_rubric_version", { rubricId: editing.id, expectedVersion: editing.version, rubric })
        : await invoke<number>("cmd_create_rubric", { rubric });
      resetEditor(); await reload();
      setNotice(`Saved rubric #${id}. Existing assignments keep their original rubric.`);
    } catch (error) { setError(String(error)); } finally { setLoading(false); }
  }
  async function createSyntheticRubric(index: number) {
    setLoading(true); setError(null); setNotice("");
    try { const id = await invoke<number>("cmd_create_rubric", { rubric: SYNTHETIC_RUBRICS[index] }); await reload(); setNotice(`Saved synthetic rubric #${id}.`); }
    catch (error) { setError(String(error)); } finally { setLoading(false); }
  }
  const total = criteria.reduce((sum, item) => sum + (Number(item.score) || 0), 0);
  return (
    <section aria-labelledby="rubrics-title">
      <h2 id="rubrics-title">Rubrics</h2>
      <p className="page-description">Create your own assessment criteria. Editing saves a new version so existing assignments and confirmed grades keep their original rubric.</p>
      <form onSubmit={saveRubric}>
        <fieldset disabled={loading} className="rubric-editor">
          <legend>{editing ? `Edit rubric #${editing.id} as version ${editing.version + 1}` : "New rubric"}</legend>
          <label>Title<input ref={titleInput} required maxLength={200} value={title} onChange={event => setTitle(event.currentTarget.value)} /></label>
          <div className="form-grid">
            <label>Subject<input required maxLength={200} value={subject} onChange={event => setSubject(event.currentTarget.value)} /></label>
            <label>Year group<input required maxLength={100} value={gradeLevel} onChange={event => setGradeLevel(event.currentTarget.value)} /></label>
          </div>
          <h3>Criteria · {Number.isFinite(total) ? total : "—"} points</h3>
          {criteria.map((item, index) => <div className="criterion-editor" key={item.key}>
            <div className="criterion-editor-fields">
              <label>Code {index + 1}<input required maxLength={32} pattern={"[A-Za-z0-9_\\-]+"} value={item.code} onChange={event => updateCriterion(item.key, "code", event.currentTarget.value)} /></label>
              <label>Description {index + 1}<textarea required maxLength={4000} value={item.description} onChange={event => updateCriterion(item.key, "description", event.currentTarget.value)} /></label>
              <label>Maximum score {index + 1}<input type="number" required min={0.01} max={10000} step="any" value={item.score} onChange={event => updateCriterion(item.key, "score", event.currentTarget.value)} /></label>
            </div>
            <button type="button" className="button-quiet" disabled={criteria.length === 1} onClick={() => setCriteria(previous => previous.filter(criterion => criterion.key !== item.key))}>Remove criterion {index + 1}</button>
          </div>)}
          <div className="action-row">
            <button type="button" disabled={criteria.length >= 100} onClick={addCriterion}>Add criterion</button>
            <button className="button-primary" type="submit">{editing ? "Save as new version" : "Save rubric"}</button>
            {editing && <button type="button" onClick={resetEditor}>Cancel editing</button>}
          </div>
        </fieldset>
      </form>
      {loading && <p role="status">Loading or saving rubrics…</p>}
      {notice && <p className="notice" role="status">{notice}</p>}
      {error && <pre role="alert">{error}</pre>}
      <details className="synthetic-rubric-tools"><summary>Synthetic examples</summary>
        <div className="action-row">{SYNTHETIC_RUBRICS.map((rubric, index) => <button key={rubric.title} disabled={loading} onClick={() => createSyntheticRubric(index)}>Create synthetic rubric: {rubric.title}</button>)}</div>
      </details>
      <h3>Saved rubrics ({rubrics.length})</h3>
      {!loading && rubrics.length === 0 && <div className="empty-state"><strong>No saved rubrics yet</strong><p>Enter your criteria above, or start with a synthetic example.</p></div>}
      <ul className="rubric-list">{rubrics.map(rubric => <li key={rubric.id}>
        <div className="submission-heading"><div><strong>#{rubric.id} — {rubric.title}</strong><p className="supporting-text">{rubric.subject} · {rubric.grade_level} · Version {rubric.version}</p></div>
          <button disabled={loading} onClick={() => edit(rubric)} aria-label={`Edit ${rubric.title} version ${rubric.version} as a new version`}>Edit as new version</button>
        </div>
        <ul>{rubric.criteria.map(criterion => <li key={criterion.id}>{criterion.code}: {criterion.description} ({criterion.score_max} pts)</li>)}</ul>
      </li>)}</ul>
    </section>
  );
}
