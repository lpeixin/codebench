import type { GeneratedCase } from "../types";

export interface DraftRow extends GeneratedCase {
  include: boolean;
}

export function CaseReview({
  provider,
  warnings,
  constraintsSummary,
  rows,
  onPatch,
  onToggle,
  onSave,
  onDiscard,
  saving,
}: {
  provider: string;
  warnings: string[];
  constraintsSummary?: string | null;
  rows: DraftRow[];
  onPatch: (i: number, patch: Partial<DraftRow>) => void;
  onToggle: (i: number, include: boolean) => void;
  onSave: () => void;
  onDiscard: () => void;
  saving: boolean;
}) {
  const selected = rows.filter((r) => r.include).length;
  const invalid = rows.filter((r) => r.validation === "invalid").length;

  return (
    <div className="panel">
      <div className="toolbar" style={{ marginTop: 0 }}>
        <b>Review generated cases</b>
        <span className={`badge ${provider.startsWith("mock") ? "mock" : "ok"}`}>{provider}</span>
        <span className="grow" />
        <span className="dim small">
          {selected} of {rows.length} selected
          {invalid > 0 ? ` · ${invalid} invalid` : ""}
        </span>
      </div>

      {warnings.map((w, i) => (
        <div key={i} className="banner warn">
          ⚠ {w}
        </div>
      ))}

      {constraintsSummary && (
        <div className="small dim" style={{ marginBottom: 10 }}>
          <b>Model's constraints summary:</b> {constraintsSummary}
        </div>
      )}

      {rows.map((row, i) => (
        <div key={i} className={`case-card ${row.include ? "" : "excluded"}`}>
          <div className="case-head">
            <label className="check">
              <input
                type="checkbox"
                checked={row.include}
                onChange={(e) => onToggle(i, e.target.checked)}
              />
              Case {i + 1}
            </label>
            {row.isEdgeCase && <span className="badge warn">edge</span>}
            <ValidationBadge row={row} />
            <span className="grow" />
            <label className="check">
              <input
                type="checkbox"
                checked={row.isEdgeCase}
                onChange={(e) => onPatch(i, { isEdgeCase: e.target.checked })}
              />
              edge case
            </label>
          </div>
          {row.invalidReason && (
            <div className="small warn-text" style={{ marginBottom: 6 }}>
              {row.invalidReason}
            </div>
          )}
          <div className="split">
            <label className="field">
              <span>stdin</span>
              <textarea
                rows={4}
                value={row.input}
                onChange={(e) => onPatch(i, { input: e.target.value })}
              />
            </label>
            <label className="field">
              <span>expected stdout</span>
              <textarea
                rows={4}
                value={row.expectedOutput}
                onChange={(e) => onPatch(i, { expectedOutput: e.target.value })}
              />
            </label>
          </div>
          {row.explanation && (
            <div className="small dim" style={{ marginBottom: 0 }}>
              {row.explanation}
            </div>
          )}
        </div>
      ))}

      <div className="toolbar">
        <button className="primary" disabled={saving || selected === 0} onClick={onSave}>
          {saving ? "Saving…" : `Save ${selected} case${selected === 1 ? "" : "s"}`}
        </button>
        <button className="plain" disabled={saving} onClick={onDiscard}>
          Discard draft
        </button>
      </div>
    </div>
  );
}

function ValidationBadge({ row }: { row: DraftRow }) {
  if (row.validation === "validated") return <span className="badge ok">verified</span>;
  if (row.validation === "invalid") return <span className="badge err">invalid</span>;
  return (
    <span className="badge dim" title="No sandbox was available to run the reference solution">
      unverified
    </span>
  );
}
