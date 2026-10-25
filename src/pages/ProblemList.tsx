import { useEffect, useState } from "react";
import { api } from "../api";
import type { Problem } from "../types";

export function ProblemList({
  onOpen,
  onImport,
}: {
  onOpen: (id: string) => void;
  onImport: () => void;
}) {
  const [problems, setProblems] = useState<Problem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Inline confirmation instead of window.confirm: native JS dialogs are not
  // reliably implemented across the three webview engines Tauri targets.
  const [confirming, setConfirming] = useState<string | null>(null);

  useEffect(() => {
    api
      .listProblems()
      .then(setProblems)
      .catch((e) => setError(String(e)));
  }, []);

  async function remove(id: string) {
    setConfirming(null);
    try {
      await api.deleteProblem(id);
      setProblems((ps) => (ps ? ps.filter((p) => p.id !== id) : ps));
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div>
      <h1 className="page-title">Problems</h1>
      <p className="page-sub">Paste any problem statement, generate test cases with a local LLM, judge locally.</p>

      {error && <div className="banner err">{error}</div>}

      <div className="toolbar">
        <button className="primary" onClick={onImport}>
          ＋ Import problem
        </button>
      </div>

      {problems === null && !error && <div className="empty">Loading…</div>}
      {problems !== null && problems.length === 0 && (
        <div className="empty">
          No problems yet. Click <b>Import problem</b> and paste a statement to get started.
        </div>
      )}

      {problems !== null && problems.length > 0 && (
        <table className="grid">
          <thead>
            <tr>
              <th>Title</th>
              <th>Language</th>
              <th>Created</th>
              <th style={{ width: "130px" }}></th>
            </tr>
          </thead>
          <tbody>
            {problems.map((p) => (
              <tr key={p.id}>
                <td>
                  <button className="link" onClick={() => onOpen(p.id)}>
                    {p.title}
                  </button>
                </td>
                <td className="dim">{p.language}</td>
                <td className="dim">{p.createdAt.slice(0, 19).replace("T", " ")}</td>
                <td>
                  {confirming === p.id ? (
                    <span className="inline-confirm">
                      <button className="danger" onClick={() => remove(p.id)}>
                        delete
                      </button>
                      <button className="link" onClick={() => setConfirming(null)}>
                        cancel
                      </button>
                    </span>
                  ) : (
                    <button className="danger" onClick={() => setConfirming(p.id)}>
                      delete
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
