import { useState } from "react";
import { api } from "../api";
import type { Example } from "../types";

export function ProblemImport({
  onCreated,
  onCancel,
}: {
  onCreated: (id: string) => void;
  onCancel: () => void;
}) {
  const [title, setTitle] = useState("");
  const [statement, setStatement] = useState("");
  const [constraints, setConstraints] = useState("");
  const [notes, setNotes] = useState("");
  const [examples, setExamples] = useState<Example[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function setExample(i: number, patch: Partial<Example>) {
    setExamples((xs) => xs.map((x, j) => (j === i ? { ...x, ...patch } : x)));
  }

  async function submit() {
    if (!title.trim() || !statement.trim()) {
      setError("Title and statement are required.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const p = await api.createProblem({
        title: title.trim(),
        statement,
        constraints: constraints.trim() || null,
        examples,
        notes: notes.trim() || null,
      });
      onCreated(p.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={{ maxWidth: 780 }}>
      <h1 className="page-title">Import problem</h1>
      <p className="page-sub">Copy a problem statement from anywhere and paste it below.</p>

      {error && <div className="banner err">{error}</div>}

      <div className="panel">
        <label className="field">
          <span>Title *</span>
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Two Sum"
          />
        </label>
        <label className="field">
          <span>Statement *</span>
          <textarea
            rows={9}
            value={statement}
            onChange={(e) => setStatement(e.target.value)}
            placeholder="Paste the full problem statement…"
          />
        </label>
        <label className="field">
          <span>Constraints</span>
          <textarea
            rows={4}
            value={constraints}
            onChange={(e) => setConstraints(e.target.value)}
            placeholder="2 &lt;= n &lt;= 10^4 …"
          />
        </label>

        <div className="field">
          <span
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              marginBottom: 5,
            }}
          >
            Examples
            <button
              className="plain"
              style={{ padding: "2px 10px", fontSize: 12 }}
              onClick={() => setExamples((xs) => [...xs, { input: "", output: "", explanation: "" }])}
            >
              ＋ add example
            </button>
          </span>
          {examples.map((ex, i) => (
            <div key={i} className="case-card">
              <div className="case-head">
                <b>Example {i + 1}</b>
                <span className="grow" />
                <button
                  className="danger"
                  onClick={() => setExamples((xs) => xs.filter((_, j) => j !== i))}
                >
                  remove
                </button>
              </div>
              <div className="split">
                <label className="field">
                  <span>Input</span>
                  <textarea
                    rows={3}
                    value={ex.input}
                    onChange={(e) => setExample(i, { input: e.target.value })}
                  />
                </label>
                <label className="field">
                  <span>Output</span>
                  <textarea
                    rows={3}
                    value={ex.output}
                    onChange={(e) => setExample(i, { output: e.target.value })}
                  />
                </label>
              </div>
              <label className="field" style={{ marginBottom: 0 }}>
                <span>Explanation (optional)</span>
                <input
                  type="text"
                  value={ex.explanation ?? ""}
                  onChange={(e) => setExample(i, { explanation: e.target.value })}
                />
              </label>
            </div>
          ))}
          {examples.length === 0 && (
            <div className="dim small">Optional — examples help the LLM generate better cases.</div>
          )}
        </div>

        <label className="field">
          <span>Notes (private)</span>
          <textarea
            rows={2}
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            placeholder="Reminders, links, approaches you want to try…"
          />
        </label>
      </div>

      <div className="toolbar">
        <button className="primary" disabled={busy} onClick={submit}>
          {busy ? "Saving…" : "Save problem"}
        </button>
        <button className="plain" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}
