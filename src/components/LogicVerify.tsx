import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { RunSummary, TestCase } from "../types";

/**
 * Logic verification flow (DESIGN.md §4.4): export the inputs, run the code
 * outside the app, paste each stdout back, compare. Nothing here executes.
 */
export function LogicVerify({
  problemId,
  cases,
  onVerified,
}: {
  problemId: string;
  cases: TestCase[];
  onVerified: (run: RunSummary) => void;
}) {
  const [exportText, setExportText] = useState<string | null>(null);
  const [outputs, setOutputs] = useState<string[]>(() => cases.map(() => ""));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const exportRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    setOutputs(cases.map(() => ""));
  }, [cases.length]);

  async function loadExport() {
    setError(null);
    try {
      setExportText(await api.exportCaseInputs(problemId));
    } catch (e) {
      setError(String(e));
    }
  }

  async function copy() {
    setError(null);
    const text = exportText ?? (await api.exportCaseInputs(problemId));
    setExportText(text);
    // Let the textarea mount before falling back to selecting it.
    await new Promise((r) => setTimeout(r, 0));
    try {
      await navigator.clipboard.writeText(text);
      flashCopied();
      return;
    } catch {
      // Webviews may deny the async clipboard API; the legacy path still works.
      const el = exportRef.current;
      if (el) {
        el.focus();
        el.select();
        try {
          if (document.execCommand("copy")) {
            flashCopied();
            return;
          }
        } catch {
          /* fall through to the message below */
        }
      }
      setError("Clipboard is unavailable — the inputs are selected, copy them manually.");
    }
  }

  function flashCopied() {
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  async function verify() {
    setBusy(true);
    setError(null);
    try {
      onVerified(await api.verifyOutputs(problemId, outputs));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="panel">
      <h3 style={{ marginTop: 0 }}>Logic verification</h3>
      <p className="small dim" style={{ marginTop: 0 }}>
        Step 1 — copy the inputs below and run your solution once per case. Step 2 — paste each
        program's stdout into the matching box. Comparison ignores trailing whitespace and blank
        lines at the end.
      </p>

      {error && <div className="banner err">{error}</div>}

      <div className="toolbar" style={{ marginTop: 0 }}>
        <button className="plain" onClick={loadExport}>
          {exportText ? "Refresh inputs" : "Show inputs"}
        </button>
        <button className="plain" onClick={copy}>
          {copied ? "Copied ✓" : "Copy inputs"}
        </button>
      </div>

      {exportText && (
        <textarea
          ref={exportRef}
          readOnly
          rows={10}
          value={exportText}
          className="export-block"
          spellCheck={false}
        />
      )}

      <h4 style={{ marginBottom: 6 }}>Paste actual outputs</h4>
      {cases.map((c, i) => (
        <div key={c.id} className="logic-case">
          <div className="case-head">
            <b>Case {i + 1}</b>
            {c.isEdgeCase && <span className="badge warn">edge</span>}
            <span className="grow" />
            <span className="dim small">
              expected: <code>{oneLine(c.expectedOutput)}</code>
            </span>
            <button
              className="link small"
              title="You ran this case outside the app and it matched"
              onClick={() =>
                setOutputs((xs) => xs.map((v, j) => (j === i ? c.expectedOutput : v)))
              }
            >
              mark passed
            </button>
          </div>
          <textarea
            rows={2}
            value={outputs[i] ?? ""}
            placeholder="paste stdout for this case"
            onChange={(e) =>
              setOutputs((xs) => xs.map((v, j) => (j === i ? e.target.value : v)))
            }
          />
        </div>
      ))}

      <div className="toolbar">
        <button className="primary" disabled={busy || cases.length === 0} onClick={verify}>
          {busy ? "Comparing…" : "Verify outputs"}
        </button>
      </div>
    </div>
  );
}

function oneLine(s: string): string {
  const flat = s.replace(/\n/g, "⏎");
  return flat.length > 40 ? `${flat.slice(0, 40)}…` : flat || "(empty)";
}
