import { useEffect, useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import type { ClassRoster } from "../lib/types";
import { useI18n } from "../lib/i18n";

/**
 * One name per line; a pasted spreadsheet column or "Surname, Name" rows also work.
 * Spacing is normalized and case-insensitive duplicates are dropped, as the backend does.
 */
export function parseRoster(text: string): string[] {
  const seen = new Set<string>();
  return text.split(/\r?\n/)
    .map(line => line.split("\t")[0].split(/\s+/).filter(Boolean).join(" "))
    .filter(name => {
      const key = name.toLocaleLowerCase();
      if (!name || seen.has(key)) return false;
      seen.add(key);
      return true;
    });
}

export function Classes() {
  const { t, errorText } = useI18n();
  const [classes, setClasses] = useState<ClassRoster[]>([]);
  const [editing, setEditing] = useState<number | null>(null);
  const [name, setName] = useState("");
  const [roster, setRoster] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("");

  async function reload() {
    setClasses(await invoke<ClassRoster[]>("cmd_list_classes"));
  }
  useEffect(() => {
    let active = true;
    invoke<ClassRoster[]>("cmd_list_classes").then(list => { if (active) setClasses(list); })
      .catch(cause => { if (active) setError(errorText(cause)); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, []);

  function edit(item: ClassRoster | null) {
    setEditing(item?.id ?? null);
    setName(item?.name ?? "");
    setRoster(item?.students.map(student => student.full_name).join("\n") ?? "");
    setError(null); setNotice("");
  }
  async function saveClass(event: FormEvent) {
    event.preventDefault();
    setLoading(true); setError(null); setNotice("");
    try {
      const students = parseRoster(roster);
      await invoke<number>("cmd_save_class", { classId: editing, name, students });
      await reload();
      edit(null);
      setNotice(t("classes.saved", { count: students.length }));
    } catch (cause) { setError(errorText(cause)); } finally { setLoading(false); }
  }
  async function deleteClass(item: ClassRoster) {
    const confirmed = await ask(t("classes.deleteConfirm", { name: item.name }), { title: t("classes.delete"), kind: "warning", okLabel: t("common.delete"), cancelLabel: t("common.cancel") });
    if (!confirmed) return;
    setLoading(true); setError(null);
    try {
      await invoke("cmd_delete_class", { classId: item.id });
      await reload();
      if (editing === item.id) edit(null);
      setNotice(t("classes.deleted"));
    } catch (cause) { setError(errorText(cause)); } finally { setLoading(false); }
  }
  const count = parseRoster(roster).length;
  return <section aria-labelledby="classes-title">
    <h2 id="classes-title">{t("classes.title")}</h2>
    <p className="page-description">{t("classes.description")}</p>
    <form onSubmit={event => void saveClass(event)}>
      <fieldset disabled={loading}>
        <legend>{editing ? t("classes.editTitle") : t("classes.newTitle")}</legend>
        <label>{t("classes.name")}<input required maxLength={100} value={name} onChange={event => setName(event.currentTarget.value)} placeholder={t("classes.namePlaceholder")} /></label>
        <label>{t("classes.roster")}<textarea rows={10} value={roster} onChange={event => setRoster(event.currentTarget.value)} placeholder={t("classes.rosterPlaceholder")} /></label>
        <p className="supporting-text">{t("classes.rosterHelp", { count })}</p>
        <div className="action-row">
          <button className="button-primary" type="submit" disabled={!name.trim()}>{t("common.save")}</button>
          {editing !== null && <button type="button" onClick={() => edit(null)}>{t("common.cancel")}</button>}
        </div>
      </fieldset>
    </form>
    {loading && <p role="status">{t("common.loading")}</p>}
    {notice && <p className="notice" role="status">{notice}</p>}
    {error && <pre role="alert">{error}</pre>}
    <h3>{t("classes.saved.title", { count: classes.length })}</h3>
    {!loading && classes.length === 0 && <div className="empty-state"><strong>{t("classes.emptyTitle")}</strong><p>{t("classes.emptyText")}</p></div>}
    <ul className="rubric-list">{classes.map(item => <li key={item.id}>
      <div className="submission-heading">
        <div><strong>{item.name}</strong><p className="supporting-text">{t("classes.count", { count: item.students.length })}</p></div>
        <div className="action-row compact">
          <button disabled={loading} onClick={() => edit(item)} aria-label={t("classes.editNamed", { name: item.name })}>{t("common.edit")}</button>
          <button className="button-quiet button-danger" disabled={loading} onClick={() => void deleteClass(item)} aria-label={t("classes.deleteNamed", { name: item.name })}>{t("common.delete")}</button>
        </div>
      </div>
      <details><summary>{t("classes.showRoster")}</summary><ul className="roster-columns">{item.students.map(student => <li key={student.id}>{student.full_name}</li>)}</ul></details>
    </li>)}</ul>
  </section>;
}
