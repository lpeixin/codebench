import { useEffect, useState } from "react";
import { api } from "../api";
import type { AiReport, AppSettings, SandboxReport } from "../types";

export function SettingsPage() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [report, setReport] = useState<SandboxReport | null>(null);
  const [ai, setAi] = useState<AiReport | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function reload() {
    try {
      const [s, r] = await Promise.all([api.getSettings(), api.detectSandbox()]);
      setSettings(s);
      setReport(r);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    reload();
  }, []);

  function patch(p: Partial<AppSettings>) {
    setSettings((s) => (s ? { ...s, ...p } : s));
  }

  async function save() {
    if (!settings) return;
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const saved = await api.updateSettings(settings);
      setSettings(saved);
      setReport(await api.detectSandbox());
      setStatus("Settings saved.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function testAi() {
    setAi(null);
    try {
      setAi(await api.checkAi());
    } catch (e) {
      setError(String(e));
    }
  }

  if (!settings) {
    return (
      <div>
        <h1 className="page-title">Settings</h1>
        {error ? <div className="banner err">{error}</div> : <div className="empty">Loading…</div>}
      </div>
    );
  }

  return (
    <div style={{ maxWidth: 680 }}>
      <h1 className="page-title">Settings</h1>
      <p className="page-sub">Everything is stored locally in the app database.</p>

      {error && <div className="banner err">{error}</div>}
      {status && <div className="banner ok">{status}</div>}

      <div className="panel">
        <h3 style={{ marginTop: 0 }}>AI (test case generation)</h3>
        <div className="form-row">
          <label className="field">
            <span>Provider mode</span>
            <select
              value={settings.aiMode}
              onChange={(e) => patch({ aiMode: e.target.value as AppSettings["aiMode"] })}
            >
              <option value="auto">auto (Ollama, fall back to mock)</option>
              <option value="ollama">ollama only</option>
              <option value="mock">mock (offline dev)</option>
            </select>
          </label>
          <label className="field">
            <span>Ollama model</span>
            <input
              type="text"
              value={settings.aiModel}
              onChange={(e) => patch({ aiModel: e.target.value })}
              placeholder="llama3.1"
            />
          </label>
        </div>
        <label className="field">
          <span>Ollama endpoint</span>
          <input
            type="text"
            value={settings.aiEndpoint}
            onChange={(e) => patch({ aiEndpoint: e.target.value })}
            placeholder="http://localhost:11434"
          />
        </label>
        <label className="field">
          <span>Number of test cases to generate</span>
          <input
            type="number"
            min={1}
            max={20}
            value={settings.numTestCases}
            onChange={(e) => patch({ numTestCases: Number(e.target.value) || 1 })}
          />
        </label>
        <button className="plain" onClick={testAi}>
          Test connection
        </button>
        {ai && (
          <div className={`banner ${ai.ok ? "ok" : "err"}`}>
            {ai.ok ? `✓ ${ai.detail}` : `✗ ${ai.detail}`}
            {ai.models.length > 0 && (
              <div className="small" style={{ marginTop: 4 }}>
                Models: {ai.models.join(", ")}
              </div>
            )}
          </div>
        )}
      </div>

      <div className="panel">
        <h3 style={{ marginTop: 0 }}>Judge &amp; sandbox</h3>
        <div className="form-row">
          <label className="field">
            <span>Sandbox mode</span>
            <select
              value={settings.sandboxMode}
              onChange={(e) =>
                patch({ sandboxMode: e.target.value as AppSettings["sandboxMode"] })
              }
            >
              <option value="auto">auto (detect)</option>
              <option value="native">native sandbox only</option>
              <option value="logic">logic verification (never execute)</option>
              <option value="mock">mock runner (dev)</option>
            </select>
          </label>
          <label className="field">
            <span>Time limit per case (seconds)</span>
            <input
              type="number"
              min={1}
              max={60}
              value={settings.timeLimitSec}
              onChange={(e) => patch({ timeLimitSec: Number(e.target.value) || 1 })}
            />
          </label>
        </div>
        <label className="field">
          <span>Memory limit (MB)</span>
          <input
            type="number"
            min={32}
            max={4096}
            value={settings.memLimitMb}
            onChange={(e) => patch({ memLimitMb: Number(e.target.value) || 32 })}
          />
        </label>
      </div>

      {report && (
        <div className="panel">
          <h3 style={{ marginTop: 0 }}>
            Current sandbox status{" "}
            <button className="link small" onClick={reload}>
              refresh
            </button>
          </h3>
          <dl className="kv">
            <dt>Requested</dt>
            <dd>{report.requestedMode}</dd>
            <dt>Backend</dt>
            <dd>
              {report.backend} {report.canExecute ? "" : "(execution disabled)"}
            </dd>
            <dt>python3</dt>
            <dd>{report.pythonAvailable ? "found" : "not found on PATH"}</dd>
            <dt>Detail</dt>
            <dd>{report.detail}</dd>
          </dl>
          {report.warnings.map((w, i) => (
            <div key={i} className="banner warn">
              ⚠ {w}
            </div>
          ))}
        </div>
      )}

      <button className="primary" disabled={busy} onClick={save}>
        {busy ? "Saving…" : "Save settings"}
      </button>
    </div>
  );
}
