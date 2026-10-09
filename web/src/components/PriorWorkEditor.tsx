import type { PriorRelation, PriorWork } from '../api/types';
import { RELATIONS, RELATION_LABELS, SOURCE_KINDS } from '../lib/labels';
import { statementTitle, type StatementLike } from './claims';
import { SourceFields } from './SourceFields';

/** An editable list of prior works, each tied to one statement. */
export function PriorWorkEditor({
  idPrefix,
  value,
  claims,
  onChange,
}: {
  idPrefix: string;
  value: PriorWork[];
  /** Statements a prior work may bear on; with one, the choice is fixed. */
  claims: readonly Pick<StatementLike, 'id' | 'kind' | 'label'>[];
  onChange: (value: PriorWork[]) => void;
}) {
  const set = (i: number, patch: Partial<PriorWork>) =>
    onChange(value.map((p, j) => (j === i ? { ...p, ...patch } : p)));
  const first = claims[0]?.id ?? '';
  return (
    <fieldset className="prior-editor">
      <legend>Prior works</legend>
      {value.length === 0 ? <p className="muted small">None listed.</p> : null}
      <ol>
        {value.map((p, i) => {
          const id = `${idPrefix}-${i}`;
          return (
            <li key={i} className="prior-row">
              <div className="form-row">
                {claims.length > 1 ? (
                  <div className="field">
                    <label htmlFor={`${id}-claim`}>Statement</label>
                    <select
                      id={`${id}-claim`}
                      value={p.claim}
                      onChange={(e) => set(i, { claim: e.target.value })}
                    >
                      {claims.map((c) => (
                        <option key={c.id} value={c.id}>
                          {c.id} · {statementTitle(c)}
                        </option>
                      ))}
                    </select>
                  </div>
                ) : null}
                <div className="field">
                  <label htmlFor={`${id}-relation`}>Relation</label>
                  <select
                    id={`${id}-relation`}
                    value={p.relation}
                    onChange={(e) => set(i, { relation: e.target.value as PriorRelation })}
                  >
                    {RELATIONS.map((r) => (
                      <option key={r} value={r}>
                        {RELATION_LABELS[r]}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
              <SourceFields
                idPrefix={id}
                value={p.source}
                kinds={SOURCE_KINDS}
                withYear
                onChange={(source) => set(i, { source })}
              />
              <div className="field">
                <label htmlFor={`${id}-note`}>Note</label>
                <input
                  id={`${id}-note`}
                  value={p.note}
                  onChange={(e) => set(i, { note: e.target.value })}
                />
              </div>
              <button
                type="button"
                className="link-button"
                onClick={() => onChange(value.filter((_, j) => j !== i))}
              >
                Remove prior work {i + 1}
              </button>
            </li>
          );
        })}
      </ol>
      <button
        type="button"
        className="button button-quiet button-small"
        onClick={() =>
          onChange([
            ...value,
            { claim: first, source: { kind: 'doi', locator: '' }, relation: 'related', note: '' },
          ])
        }
      >
        Add prior work
      </button>
    </fieldset>
  );
}
