import type { CaseResult, RunSummary } from "../types";

const LABELS: Record<CaseResult["status"], string> = {
  passed: "passed",
  wrong_answer: "wrong answer",
  timeout: "timeout",
  runtime_error: "runtime error",
  internal: "internal error",
};

export function RunResults({ run }: { run: RunSummary }) {
  const passed = run.results.filter((r) => r.status === "passed").length;

  return (
    <div>
      <div className="mode-line">
        <span className={`badge ${run.mode === "mock" ? "mock" : run.mode === "logic" ? "warn" : "ok"}`}>
          {run.mode === "mock" ? "MOCK" : run.mode === "logic" ? "LOGIC" : "SANDBOX"}
        </span>{" "}
        <b className={`status-${run.status}`}>{run.status}</b> · {passed}/{run.results.length} cases
        passed · {run.startedAt.replace("T", " ").replace("Z", "")}
      </div>

      {run.results.map((r) => (
        <details key={r.caseId + r.idx} className={`result-row ${r.status}`} open={r.status !== "passed"}>
          <summary>
            <span className="result-idx">#{r.idx + 1}</span>
            <span className={`status-${r.status}`}>{LABELS[r.status]}</span>
            <span className="grow" />
            {r.timeMs != null && <span className="dim small">{r.timeMs} ms</span>}
          </summary>
          <div className="split" style={{ marginTop: 8 }}>
            <div>
              <div className="small dim">expected</div>
              <pre className="code">{r.expected || "(empty)"}</pre>
            </div>
            <div>
              <div className="small dim">actual</div>
              <pre className="code">{r.actual || "(empty)"}</pre>
            </div>
          </div>
          {r.stderr && (
            <div>
              <div className="small dim">stderr</div>
              <pre className="code stderr">{r.stderr}</pre>
            </div>
          )}
        </details>
      ))}
    </div>
  );
}
