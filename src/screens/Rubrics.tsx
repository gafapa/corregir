import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SYNTHETIC_RUBRICS, RubricWithCriteria } from "../lib/types";

/**
 * Milestone 2: create and list rubrics in the encrypted database. Manual
 * check: create both synthetic rubrics, restart the app, and verify that
 * they still appear in the list.
 */
export function Rubrics() {
  const [rubrics, setRubrics] = useState<RubricWithCriteria[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function reload() {
    setError(null);
    try {
      const list = await invoke<RubricWithCriteria[]>("cmd_list_rubrics");
      setRubrics(list);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    reload();
  }, []);

  async function createSyntheticRubric(index: number) {
    setLoading(true);
    setError(null);
    try {
      await invoke<number>("cmd_create_rubric", {
        rubric: SYNTHETIC_RUBRICS[index],
      });
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <section>
      <h2>Rubrics (Milestone 2)</h2>
      <p>
        Rubrics contain configuration data, not student data, so they do not
        require redaction.
      </p>

      <div className="row">
        {SYNTHETIC_RUBRICS.map((r, i) => (
          <button key={r.title} onClick={() => createSyntheticRubric(i)} disabled={loading}>
            Create synthetic rubric: {r.title}
          </button>
        ))}
      </div>

      {error && <pre role="alert" style={{ color: "crimson" }}>{error}</pre>}

      <h3>Saved rubrics ({rubrics.length})</h3>
      <ul>
        {rubrics.map((r) => (
          <li key={r.id}>
            <strong>
              #{r.id} — {r.title}
            </strong>{" "}
            ({r.subject}, {r.grade_level})
            <ul>
              {r.criteria.map((c) => (
                <li key={c.id}>
                  {c.code}: {c.description} ({c.score_max} pts)
                </li>
              ))}
            </ul>
          </li>
        ))}
      </ul>
    </section>
  );
}
