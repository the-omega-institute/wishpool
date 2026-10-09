import { useState, type FormEvent } from 'react';
import { useApi } from '../../api/context';
import type { Claim, ClaimKind, ClaimRole, Source, Submission } from '../../api/types';
import {
  buildConfirmations,
  initialRows,
  isMainResult,
  toggleDependency,
  updateRow,
  type ConfirmRow,
} from '../../lib/confirm';
import { CLAIM_KINDS, CLAIM_KIND_LABELS, SOURCE_KINDS } from '../../lib/labels';
import { statementTitle } from '../claims';
import { LatexText } from '../Markdown';
import { SourceFields } from '../SourceFields';
import { Badge, InlineError } from '../ui';

/**
 * S1: the author confirms each statement the server read from the source —
 * kind, main result or supporting, dependencies — or excludes it.
 */
export function ConfirmStatements({
  submission,
  onChange,
}: {
  submission: Submission;
  onChange: (s: Submission) => void;
}) {
  const api = useApi();
  const extracted = submission.extracted;
  const [rows, setRows] = useState<ConfirmRow[]>(() => initialRows(extracted, submission.claims));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);

  if (extracted.length === 0) {
    return (
      <div className="notice notice-error" role="alert">
        <p>
          No theorem-like statements were found in this version. Check that the paper uses
          environments such as <code>theorem</code>, <code>lemma</code> or <code>conjecture</code>{' '}
          declared with <code>\newtheorem</code>, then upload a new version.
        </p>
      </div>
    );
  }

  const mainCount = rows.filter(isMainResult).length;
  const excludedCount = rows.filter((r) => r.excluded).length;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const built = buildConfirmations(rows);
    if (!built.ok) {
      setError(built.error);
      return;
    }
    setBusy(true);
    setError(null);
    api.confirmClaims(submission.id, built.value).then(
      (s) => {
        setBusy(false);
        onChange(s);
      },
      (e: unknown) => {
        setBusy(false);
        setError(e);
      },
    );
  };

  return (
    <form className="confirm-form" onSubmit={submit} aria-label="Confirm statements">
      <p>
        The server read {extracted.length} statement{extracted.length === 1 ? '' : 's'} from the
        source. For each, check the kind, mark whether it is a main result of the paper or
        supporting, and select the statements its proof uses. Exclude anything that was misparsed.
        At least one proved statement must be a main result; conjectures and questions are never
        main results.
      </p>
      <div className="table-wrap">
        <table className="data-table confirm-table">
          <caption className="visually-hidden">Extracted statements</caption>
          <thead>
            <tr>
              <th scope="col">Statement</th>
              <th scope="col" className="confirm-kind">
                Kind
              </th>
              <th scope="col" className="confirm-role">
                Role
              </th>
              <th scope="col" className="confirm-deps">
                Uses
              </th>
              <th scope="col" className="confirm-exclude">
                Exclude
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => {
              const claim = extracted.find((c) => c.id === row.id) as Claim;
              return (
                <ConfirmRowView
                  key={row.id}
                  row={row}
                  claim={claim}
                  rows={rows}
                  extracted={extracted}
                  onPatch={(patch) => setRows((rs) => updateRow(rs, row.id, patch))}
                  onDependency={(dep, on) => setRows((rs) => toggleDependency(rs, row.id, dep, on))}
                />
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="small" aria-live="polite">
        {mainCount} main result{mainCount === 1 ? '' : 's'} · {excludedCount} excluded ·{' '}
        {rows.length - excludedCount} to review
      </p>
      <div className="button-row">
        <button type="submit" className="button" disabled={busy}>
          {busy ? 'Confirming…' : 'Confirm statements and start review'}
        </button>
      </div>
      <InlineError error={error} />
    </form>
  );
}

function ConfirmRowView({
  row,
  claim,
  rows,
  extracted,
  onPatch,
  onDependency,
}: {
  row: ConfirmRow;
  claim: Claim;
  rows: readonly ConfirmRow[];
  extracted: readonly Claim[];
  onPatch: (patch: Partial<Omit<ConfirmRow, 'id'>>) => void;
  onDependency: (dep: string, on: boolean) => void;
}) {
  const others = extracted.filter((c) => c.id !== row.id);
  const id = (part: string) => `confirm-${row.id}-${part}`;
  return (
    <tr className={row.excluded ? 'row-excluded' : undefined}>
      <td className="confirm-statement">
        <div className="claim-head">
          <span className="claim-label">{statementTitle(claim)}</span>
          <code className="claim-id">{row.id}</code>
          {claim.latex_label ? <code className="muted small">{claim.latex_label}</code> : null}
          {!claim.has_proof && row.kind !== 'conjecture' && row.kind !== 'question' ? (
            <Badge tone="muted" title="No proof environment follows this statement in the source">
              No proof in source
            </Badge>
          ) : null}
        </div>
        {claim.section ? <p className="muted small">§ {claim.section}</p> : null}
        <LatexText source={claim.statement} className="statement" />
        <SettlesEditor row={row} onPatch={onPatch} />
      </td>
      <td className="confirm-kind" data-label="Kind">
        <label className="visually-hidden" htmlFor={id('kind')}>
          Kind of {row.id}
        </label>
        <select
          id={id('kind')}
          value={row.kind}
          disabled={row.excluded}
          onChange={(e) => onPatch({ kind: e.target.value as ClaimKind })}
        >
          {CLAIM_KINDS.map((k) => (
            <option key={k} value={k}>
              {CLAIM_KIND_LABELS[k]}
            </option>
          ))}
        </select>
      </td>
      <td className="confirm-role" data-label="Role">
        <label className="visually-hidden" htmlFor={id('role')}>
          Role of {row.id}
        </label>
        <select
          id={id('role')}
          value={row.role}
          disabled={row.excluded}
          onChange={(e) => onPatch({ role: e.target.value as ClaimRole })}
        >
          <option value="main">Main result</option>
          <option value="supporting">Supporting</option>
        </select>
      </td>
      <td className="confirm-deps" data-label="Uses">
        {others.length === 0 ? (
          <span className="muted small">—</span>
        ) : (
          <details className="dep-picker">
            <summary>
              {row.dependsOn.length === 0 ? 'None' : row.dependsOn.join(', ')}
              <span className="visually-hidden"> — statements {row.id} uses</span>
            </summary>
            <fieldset disabled={row.excluded}>
              <legend className="visually-hidden">Statements {row.id} uses</legend>
              {others.map((c) => {
                const excluded = rows.find((r) => r.id === c.id)?.excluded ?? false;
                return (
                  <label key={c.id} className="checkbox">
                    <input
                      type="checkbox"
                      aria-label={`${row.id} uses ${c.id}`}
                      checked={row.dependsOn.includes(c.id)}
                      onChange={(e) => onDependency(c.id, e.target.checked)}
                    />
                    <span className={excluded ? 'muted' : undefined}>
                      {c.id} · {statementTitle(c)}
                      {excluded ? ' (excluded)' : ''}
                    </span>
                  </label>
                );
              })}
            </fieldset>
          </details>
        )}
      </td>
      <td className="confirm-exclude" data-label="Exclude">
        <label className="checkbox">
          <input
            type="checkbox"
            checked={row.excluded}
            onChange={(e) => onPatch({ excluded: e.target.checked })}
          />
          <span>
            <span className="visually-hidden">Exclude {row.id}</span>
            <span aria-hidden="true" className="confirm-exclude-text">
              Misparsed
            </span>
          </span>
        </label>
      </td>
    </tr>
  );
}

const EMPTY_SOURCE: Source = { kind: 'doi', locator: '' };

/** Optional: the statement settles a named, sourced open problem. */
function SettlesEditor({
  row,
  onPatch,
}: {
  row: ConfirmRow;
  onPatch: (patch: Partial<Omit<ConfirmRow, 'id'>>) => void;
}) {
  const [open, setOpen] = useState(Boolean(row.settles));
  if (row.excluded) return null;
  const settles = row.settles ?? { name: '', source: EMPTY_SOURCE };
  return (
    <div className="settles-editor">
      <label className="checkbox small">
        <input
          type="checkbox"
          checked={open}
          onChange={(e) => {
            setOpen(e.target.checked);
            if (!e.target.checked) onPatch({ settles: undefined });
          }}
        />
        <span>Settles a named open problem</span>
      </label>
      {open ? (
        <div className="settles-fields">
          <div className="field">
            <label htmlFor={`settles-${row.id}-name`}>Name of the problem</label>
            <input
              id={`settles-${row.id}-name`}
              value={settles.name}
              onChange={(e) => onPatch({ settles: { ...settles, name: e.target.value } })}
            />
          </div>
          <SourceFields
            idPrefix={`settles-${row.id}`}
            value={settles.source}
            kinds={SOURCE_KINDS}
            withYear
            onChange={(source) => onPatch({ settles: { ...settles, source } })}
          />
        </div>
      ) : null}
    </div>
  );
}
