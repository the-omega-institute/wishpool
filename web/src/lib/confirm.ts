import type { Claim, ClaimConfirmation, ClaimKind, ClaimRole, Settles } from '../api/types';
import { isOpenKind } from './labels';
import { fail, ok, type Parsed } from './parsed';

/** The author's working copy of one extracted statement. */
export interface ConfirmRow {
  id: string;
  kind: ClaimKind;
  role: ClaimRole;
  dependsOn: string[];
  excluded: boolean;
  conjectureInput?: string;
  settles?: Settles;
}

/** Start from what the server read; a previous confirmation of the same ids is kept. */
export function initialRows(
  extracted: readonly Claim[],
  confirmed: readonly Claim[] = [],
  dependencies: { from: string; target: { record: string; claim: string } }[] = [],
): ConfirmRow[] {
  return extracted.map((c) => {
    const earlier = confirmed.find((x) => x.id === c.id);
    const source = earlier ?? c;
    return {
      ...(dependencies.some((d) => d.from === c.id)
        ? {
            conjectureInput: dependencies
              .filter((d) => d.from === c.id)
              .map((d) => `${d.target.record}:${d.target.claim}`)
              .join(', '),
          }
        : {}),
      id: c.id,
      kind: source.kind,
      role: source.role,
      dependsOn: source.depends_on.filter((d) => extracted.some((e) => e.id === d)),
      excluded: false,
      settles: source.settles ?? undefined,
    };
  });
}

/** A statement the server counts as a main result: marked main and not a conjecture or question. */
export function isMainResult(row: Pick<ConfirmRow, 'kind' | 'role' | 'excluded'>): boolean {
  return !row.excluded && row.role === 'main' && !isOpenKind(row.kind);
}

export function updateRow(
  rows: readonly ConfirmRow[],
  id: string,
  patch: Partial<Omit<ConfirmRow, 'id'>>,
): ConfirmRow[] {
  return rows.map((r) => (r.id === id ? { ...r, ...patch } : r));
}

export function toggleDependency(
  rows: readonly ConfirmRow[],
  id: string,
  dep: string,
  on: boolean,
) {
  return rows.map((r) => {
    if (r.id !== id) return r;
    const set = new Set(r.dependsOn);
    if (on) set.add(dep);
    else set.delete(dep);
    // Keep the paper's order.
    return { ...r, dependsOn: rows.map((x) => x.id).filter((x) => set.has(x)) };
  });
}

function hasCycle(rows: readonly ConfirmRow[]): string | null {
  const edges = new Map(rows.map((r) => [r.id, r.dependsOn]));
  const marks = new Map<string, 'visiting' | 'done'>();
  const visit = (id: string): boolean => {
    const mark = marks.get(id);
    if (mark === 'done') return false;
    if (mark === 'visiting') return true;
    marks.set(id, 'visiting');
    for (const d of edges.get(id) ?? []) if (visit(d)) return true;
    marks.set(id, 'done');
    return false;
  };
  for (const r of rows) if (visit(r.id)) return r.id;
  return null;
}

/**
 * The body of `POST /submissions/{id}/claims`: one confirmation per
 * extracted statement, excluded ones included with `excluded: true`.
 * Checks what the server will check, so the author sees it before sending.
 */
export function buildConfirmations(
  rows: readonly ConfirmRow[],
  conjecture = false,
): Parsed<ClaimConfirmation[]> {
  const kept = rows.filter((r) => !r.excluded);
  if (kept.length === 0) return fail('Keep at least one statement.');
  const keptIds = new Set(kept.map((r) => r.id));
  for (const r of kept) {
    if (
      r.conjectureInput?.trim() &&
      r.conjectureInput
        .split(/[\s,]+/)
        .filter(Boolean)
        .some((x) => !/^WP-\d{4}-\d{4,}:[A-Za-z0-9_-]{1,32}$/.test(x))
    )
      return fail(`Use record:claim references such as WP-2026-0001:C1 for ${r.id}.`);
    if (r.dependsOn.includes(r.id)) return fail(`${r.id} cannot depend on itself.`);
    const missing = r.dependsOn.find((d) => !keptIds.has(d));
    if (missing !== undefined) return fail(`${r.id} depends on ${missing}, which is excluded.`);
    if (r.settles && (r.settles.name.trim() === '' || r.settles.source.locator.trim() === '')) {
      return fail(`Name the open problem ${r.id} settles and give its source.`);
    }
  }
  const cyclic = hasCycle(kept);
  if (cyclic !== null) return fail(`The dependencies form a cycle through ${cyclic}.`);
  if (!kept.some((r) => (conjecture ? r.role === 'main' && isOpenKind(r.kind) : isMainResult(r)))) {
    return fail(
      'Mark at least one proved statement (not a conjecture or question) as a main result.',
    );
  }
  return ok(
    rows.map((r): ClaimConfirmation => {
      if (r.excluded) return { id: r.id, kind: r.kind, role: r.role, excluded: true };
      const c: ClaimConfirmation = {
        id: r.id,
        kind: r.kind,
        role: r.role,
        depends_on: r.dependsOn,
      };
      if (r.conjectureInput?.trim())
        c.depends_on_conjectures = r.conjectureInput
          .split(/[\s,]+/)
          .filter(Boolean)
          .map((x) => {
            const [record, claim] = x.split(':');
            return { record: record!, claim: claim! };
          });
      if (r.settles) {
        c.settles = {
          name: r.settles.name.trim(),
          source: { ...r.settles.source, locator: r.settles.source.locator.trim() },
        };
      }
      return c;
    }),
  );
}
