import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { CaseReview, type DraftRow } from "../components/CaseReview";
import { CodeEditor } from "../components/CodeEditor";
import { LogicVerify } from "../components/LogicVerify";
import { RunResults } from "../components/RunResults";
import { SandboxBanner } from "../components/SandboxBanner";
import type {
  GenerationDraft,
  ProblemDetail as ProblemDetailDto,
  ReferenceSolution,
  RunSummary,
  SandboxReport,
  SaveCaseInput,
} from "../types";

type Tab = "statement" | "tests" | "solution" | "results";

interface DraftState {
  provider: string;
  warnings: string[];
  constraintsSummary: string | null;
  reference: ReferenceSolution | null;
  rows: DraftRow[];
}

export function ProblemDetail({
  problemId,
  onDeleted,
}: {
  problemId: string;
  onDeleted: () => void;
}) {
  const [detail, setDetail] = useState<ProblemDetailDto | null>(null);
  const [report, setReport] = useState<SandboxReport | null>(null);
  const [tab, setTab] = useState<Tab>("statement");
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const d = await api.getProblem(problemId);
      setDetail(d);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [problemId]);

  useEffect(() => {
    reload();
    api.detectSandbox().then(setReport).catch((e) => setError(String(e)));
  }, [reload]);

  if (!detail) {
    return (
      <div>
        <h1 className="page-title">Loading…</h1>
        {error && <div className="banner err">{error}</div>}
      </div>
    );
  }

  const { problem } = detail;
  const canExecute = report?.canExecute ?? false;

  return (
    <div>
      <div className="toolbar" style={{ marginTop: 0 }}>
        <div>
          <h1 className="page-title" style={{ marginBottom: 2 }}>
            {problem.title}
          </h1>
          <div className="dim small">
            {problem.language} · created {problem.createdAt.slice(0, 10)} · {detail.testCases.length}{" "}
            test case{detail.testCases.length === 1 ? "" : "s"}
            {detail.lastRun && (
              <>
                {" "}
                · last run <span className={`status-${detail.lastRun.status}`}>{detail.lastRun.status}</span>
              </>
            )}
          </div>
        </div>
        <span className="grow" />
        <button className="danger" onClick={onDeleted}>
          delete
        </button>
      </div>

      {error && <div className="banner err">{error}</div>}
      {report && <SandboxBanner report={report} />}

      <div className="tabs">
        <button className={tab === "statement" ? "active" : ""} onClick={() => setTab("statement")}>
          Statement
        </button>
        <button className={tab === "tests" ? "active" : ""} onClick={() => setTab("tests")}>
          Test cases ({detail.testCases.length})
        </button>
        <button className={tab === "solution" ? "active" : ""} onClick={() => setTab("solution")}>
          Solution
        </button>
        <button className={tab === "results" ? "active" : ""} onClick={() => setTab("results")}>
          Run
          {detail.lastRun && (
            <span className={`status-${detail.lastRun.status}`}> · {detail.lastRun.status}</span>
          )}
        </button>
      </div>

      {tab === "statement" && (
        <StatementPanel detail={detail} onSaved={reload} onError={setError} />
      )}

      {tab === "tests" && (
        <TestCasesPanel
          detail={detail}
          onReload={reload}
          onError={setError}
          onGoToSolution={() => setTab("solution")}
        />
      )}

      {tab === "solution" && (
        <SolutionPanel
          detail={detail}
          canExecute={canExecute}
          onReload={reload}
          onError={setError}
          onRan={(run) => {
            setDetail((d) => (d ? { ...d, lastRun: run } : d));
            setTab("results");
          }}
        />
      )}

      {tab === "results" && (
        <div>
          {detail.lastRun ? (
            <RunResults run={detail.lastRun} />
          ) : (
            <div className="empty">No run yet. Write a solution and press Run.</div>
          )}
          {!canExecute && detail.testCases.length > 0 && (
            <LogicVerify
              problemId={problemId}
              cases={detail.testCases}
              onVerified={(run: RunSummary) => {
                setDetail((d) => (d ? { ...d, lastRun: run } : d));
              }}
            />
          )}
          {!canExecute && detail.testCases.length === 0 && (
            <div className="banner warn">
              Add or generate test cases first — logic verification needs something to compare
              against.
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Statement
// ---------------------------------------------------------------------------

function StatementPanel({
  detail,
  onSaved,
  onError,
}: {
  detail: ProblemDetailDto;
  onSaved: () => Promise<void>;
  onError: (e: string | null) => void;
}) {
  const { problem } = detail;
  const [editing, setEditing] = useState(false);
  const [form, setForm] = useState({
    title: problem.title,
    statement: problem.statement,
    constraints: problem.constraints ?? "",
    notes: problem.notes ?? "",
  });
  const [busy, setBusy] = useState(false);

  async function save() {
    setBusy(true);
    onError(null);
    try {
      await api.updateProblem(problem.id, {
        title: form.title,
        statement: form.statement,
        constraints: form.constraints || null,
        notes: form.notes || null,
      });
      setEditing(false);
      await onSaved();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }

  if (editing) {
    return (
      <div className="panel">
        <label className="field">
          <span>Title</span>
          <input value={form.title} onChange={(e) => setForm({ ...form, title: e.target.value })} />
        </label>
        <label className="field">
          <span>Statement</span>
          <textarea
            rows={12}
            value={form.statement}
            onChange={(e) => setForm({ ...form, statement: e.target.value })}
          />
        </label>
        <label className="field">
          <span>Constraints</span>
          <textarea
            rows={4}
            value={form.constraints}
            onChange={(e) => setForm({ ...form, constraints: e.target.value })}
          />
        </label>
        <label className="field">
          <span>Notes (private)</span>
          <textarea
            rows={3}
            value={form.notes}
            onChange={(e) => setForm({ ...form, notes: e.target.value })}
          />
        </label>
        <div className="toolbar">
          <button className="primary" disabled={busy} onClick={save}>
            {busy ? "Saving…" : "Save"}
          </button>
          <button className="plain" onClick={() => setEditing(false)}>
            Cancel
          </button>
        </div>
      </div>
    );
  }

  return (
    <div>
      <div className="toolbar" style={{ marginTop: 0 }}>
        <button className="plain" onClick={() => setEditing(true)}>
          Edit statement
        </button>
      </div>

      <div className="panel">
        <div className="prose">{problem.statement}</div>
      </div>

      {problem.constraints && (
        <div className="panel">
          <h3 style={{ marginTop: 0 }}>Constraints</h3>
          <pre className="code">{problem.constraints}</pre>
        </div>
      )}

      {problem.examples.length > 0 && (
        <div className="panel">
          <h3 style={{ marginTop: 0 }}>Examples</h3>
          {problem.examples.map((ex, i) => (
            <div key={i} className="case-card">
              <div className="case-head">
                <b>Example {i + 1}</b>
              </div>
              <div className="split">
                <div>
                  <div className="small dim">input</div>
                  <pre className="code">{ex.input}</pre>
                </div>
                <div>
                  <div className="small dim">output</div>
                  <pre className="code">{ex.output}</pre>
                </div>
              </div>
              {ex.explanation && <div className="small dim">{ex.explanation}</div>}
            </div>
          ))}
        </div>
      )}

      {problem.notes && (
        <div className="panel">
          <h3 style={{ marginTop: 0 }}>Your notes</h3>
          <div className="prose">{problem.notes}</div>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

function TestCasesPanel({
  detail,
  onReload,
  onError,
  onGoToSolution,
}: {
  detail: ProblemDetailDto;
  onReload: () => Promise<void>;
  onError: (e: string | null) => void;
  onGoToSolution: () => void;
}) {
  const [draft, setDraft] = useState<DraftState | null>(null);
  const [generating, setGenerating] = useState(false);
  const [saving, setSaving] = useState(false);
  const [manual, setManual] = useState({ input: "", expected: "" });
  const [showReference, setShowReference] = useState(false);

  async function generate() {
    setGenerating(true);
    onError(null);
    try {
      const g: GenerationDraft = await api.generateCases(detail.problem.id);
      setDraft({
        provider: g.provider,
        warnings: g.warnings,
        constraintsSummary: g.constraintsSummary ?? null,
        reference: g.referenceSolution ?? null,
        rows: g.cases.map((c) => ({
          ...c,
          include: c.validation !== "invalid",
        })),
      });
    } catch (e) {
      onError(String(e));
    } finally {
      setGenerating(false);
    }
  }

  async function saveDraft() {
    if (!draft) return;
    setSaving(true);
    onError(null);
    try {
      const cases: SaveCaseInput[] = draft.rows
        .filter((r) => r.include)
        .map((r) => ({
          input: r.input,
          expectedOutput: r.expectedOutput,
          explanation: r.explanation ?? null,
          isEdgeCase: r.isEdgeCase,
        }));
      await api.saveTestCases(
        detail.problem.id,
        draft.provider.startsWith("mock") ? "mock" : "ai",
        cases,
        draft.reference,
      );
      setDraft(null);
      await onReload();
    } catch (e) {
      onError(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function addManual() {
    if (!manual.expected.trim()) {
      onError("Manual cases need an expected output (it is what the judge compares against).");
      return;
    }
    onError(null);
    try {
      const existing: SaveCaseInput[] = detail.testCases
        .filter((c) => c.source === "manual")
        .map((c) => ({
          input: c.input,
          expectedOutput: c.expectedOutput,
          explanation: c.explanation ?? null,
          isEdgeCase: c.isEdgeCase,
        }));
      await api.saveTestCases(
        detail.problem.id,
        "manual",
        [...existing, { input: manual.input, expectedOutput: manual.expected, explanation: null, isEdgeCase: false }],
        null,
      );
      setManual({ input: "", expected: "" });
      await onReload();
    } catch (e) {
      onError(String(e));
    }
  }

  return (
    <div>
      {!draft && (
        <div className="toolbar" style={{ marginTop: 0 }}>
          <button className="primary" disabled={generating} onClick={generate}>
            {generating ? "Generating…" : "Generate cases"}
          </button>
          <span className="dim small">
            Uses the provider from Settings. Review before anything is saved.
          </span>
        </div>
      )}

      {draft && (
        <CaseReview
          provider={draft.provider}
          warnings={draft.warnings}
          constraintsSummary={draft.constraintsSummary}
          rows={draft.rows}
          saving={saving}
          onPatch={(i, patch) =>
            setDraft((d) =>
              d ? { ...d, rows: d.rows.map((r, j) => (j === i ? { ...r, ...patch } : r)) } : d,
            )
          }
          onToggle={(i, include) =>
            setDraft((d) =>
              d ? { ...d, rows: d.rows.map((r, j) => (j === i ? { ...r, include } : r)) } : d,
            )
          }
          onSave={saveDraft}
          onDiscard={() => setDraft(null)}
        />
      )}

      {!draft && detail.testCases.length === 0 && (
        <div className="empty">
          No test cases yet. Generate them with a local model, or add one by hand below.
        </div>
      )}

      {!draft &&
        detail.testCases.map((c, i) => (
          <div key={c.id} className="case-card">
            <div className="case-head">
              <b>Case {i + 1}</b>
              {c.isEdgeCase && <span className="badge warn">edge</span>}
              <span className={`badge ${c.source === "manual" ? "dim" : c.source === "mock" ? "mock" : "ok"}`}>
                {c.source}
              </span>
              <span className="grow" />
              <span className="dim small mono">idx {c.idx}</span>
            </div>
            <div className="split">
              <div>
                <div className="small dim">stdin</div>
                <pre className="code">{c.input || "(empty)"}</pre>
              </div>
              <div>
                <div className="small dim">expected stdout</div>
                <pre className="code">{c.expectedOutput}</pre>
              </div>
            </div>
            {c.explanation && <div className="small dim">{c.explanation}</div>}
          </div>
        ))}

      {!draft && (
        <div className="panel">
          <h3 style={{ marginTop: 0 }}>Add a case by hand</h3>
          <div className="split">
            <label className="field">
              <span>stdin</span>
              <textarea
                rows={3}
                value={manual.input}
                onChange={(e) => setManual({ ...manual, input: e.target.value })}
              />
            </label>
            <label className="field">
              <span>expected stdout</span>
              <textarea
                rows={3}
                value={manual.expected}
                onChange={(e) => setManual({ ...manual, expected: e.target.value })}
              />
            </label>
          </div>
          <div className="toolbar">
            <button className="plain" onClick={addManual}>
              Add case
            </button>
            {detail.testCases.length > 0 && (
              <button className="link" onClick={onGoToSolution}>
                go to solution →
              </button>
            )}
          </div>
        </div>
      )}

      {detail.reference && !draft && (
        <div className="panel">
          <div className="toolbar" style={{ marginTop: 0 }}>
            <b>Reference solution</b>
            <span className="badge dim">{detail.reference.language}</span>
            <span className="grow" />
            <button className="plain" onClick={() => setShowReference((v) => !v)}>
              {showReference ? "hide" : "show"}
            </button>
          </div>
          {showReference && (
            <>
              <div className="small dim" style={{ marginBottom: 6 }}>
                Generated by the model and used to validate the cases. Reading it before you solve
                the problem defeats the point — your call.
              </div>
              <pre className="code">{detail.reference.code}</pre>
            </>
          )}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Solution
// ---------------------------------------------------------------------------

function SolutionPanel({
  detail,
  canExecute,
  onReload,
  onError,
  onRan,
}: {
  detail: ProblemDetailDto;
  canExecute: boolean;
  onReload: () => Promise<void>;
  onError: (e: string | null) => void;
  onRan: (run: RunSummary) => void;
}) {
  const [code, setCode] = useState(detail.solution?.code ?? DEFAULT_TEMPLATE);
  const [saved, setSaved] = useState<string | null>(
    detail.solution ? `saved ${detail.solution.updatedAt.replace("T", " ").replace("Z", "")}` : null,
  );
  const [running, setRunning] = useState(false);
  const dirty = useRef(false);
  const problemId = detail.problem.id;

  useEffect(() => {
    if (!dirty.current) return;
    const timer = setTimeout(async () => {
      try {
        await api.saveSolution(problemId, "python", code);
        dirty.current = false;
        setSaved(`saved ${new Date().toLocaleTimeString()}`);
      } catch (e) {
        onError(String(e));
      }
    }, 900);
    return () => clearTimeout(timer);
  }, [code, problemId, onError]);

  async function saveNow() {
    try {
      await api.saveSolution(problemId, "python", code);
      dirty.current = false;
      setSaved(`saved ${new Date().toLocaleTimeString()}`);
      await onReload();
    } catch (e) {
      onError(String(e));
    }
  }

  async function run() {
    setRunning(true);
    onError(null);
    try {
      // Persist first: the judge reads the solution from the database, and a
      // run that judged stale code would be a lie.
      await api.saveSolution(problemId, "python", code);
      dirty.current = false;
      onRan(await api.runSolution(problemId));
    } catch (e) {
      onError(String(e));
    } finally {
      setRunning(false);
    }
  }

  return (
    <div>
      <div className="toolbar" style={{ marginTop: 0 }}>
        <span className="dim small">{saved ?? "not saved yet"}</span>
        <span className="grow" />
        <button className="plain" onClick={saveNow}>
          Save
        </button>
        <button className="primary" disabled={running || !canExecute} onClick={run}>
          {running ? "Running…" : "Run against test cases"}
        </button>
      </div>

      {!canExecute && (
        <div className="banner warn">
          Local judging is disabled, so <b>Run</b> would have nothing safe to execute with. Open the
          Run tab: it exports the inputs so you can run the code yourself and paste the outputs
          back.
        </div>
      )}

      <div className="editor-wrap">
        <CodeEditor
          value={code}
          height="440px"
          onChange={(v) => {
            dirty.current = true;
            setCode(v);
          }}
        />
      </div>

      <div className="small dim" style={{ marginTop: 8 }}>
        A solution is a complete program: read the case input from stdin, print the answer to
        stdout.
      </div>
    </div>
  );
}

const DEFAULT_TEMPLATE = `import sys


def main() -> None:
    data = sys.stdin.read().split()
    # TODO: implement


if __name__ == "__main__":
    main()
`;
