import type { SandboxReport } from "../types";

/**
 * Always rendered before anything can execute. It states which backend will be
 * used and — when code will not run — exactly why, because "the button is
 * disabled" is not an explanation.
 */
export function SandboxBanner({ report }: { report: SandboxReport }) {
  const mock = report.backend === "mock";
  const ok = report.canExecute && !mock;

  return (
    <div className={`mode-banner ${mock ? "mock" : ok ? "ok" : "warn"}`}>
      <div className="mode-head">
        <span className={`badge ${mock ? "mock" : ok ? "ok" : "warn"}`}>
          {mock ? "MOCK" : ok ? "SANDBOX" : "LOGIC MODE"}
        </span>
        <b>{report.backend}</b>
        <span className="dim small">
          requested: {report.requestedMode} · python {report.pythonAvailable ? "found" : "missing"}
        </span>
      </div>
      <div className="small">{report.detail}</div>
      {!report.canExecute && (
        <div className="small" style={{ marginTop: 4 }}>
          Your code is <b>never executed</b> by this app in this mode. Use the Run tab to run it
          yourself and paste the outputs back.
        </div>
      )}
      {mock && (
        <div className="small" style={{ marginTop: 4 }}>
          Results are simulated: every case is reported as passed by construction.
        </div>
      )}
      {report.warnings.map((w, i) => (
        <div key={i} className="small warn-text" style={{ marginTop: 4 }}>
          ⚠ {w}
        </div>
      ))}
    </div>
  );
}
