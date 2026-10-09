import { describe, expect, it } from 'vitest';
import { claims, extracted } from '../test/fixtures';
import { buildConfirmations, initialRows, toggleDependency, updateRow } from './confirm';

describe('confirming statements', () => {
  it('starts from the extracted statements', () => {
    const rows = initialRows(extracted);
    expect(rows.map((r) => [r.id, r.kind, r.role, r.excluded])).toEqual([
      ['C1', 'theorem', 'main', false],
      ['C2', 'lemma', 'supporting', false],
      ['C3', 'conjecture', 'supporting', false],
      ['C4', 'claim', 'supporting', false],
    ]);
  });

  it('keeps an earlier confirmation of the same ids', () => {
    const rows = initialRows(extracted, claims);
    expect(rows[0]?.dependsOn).toEqual(['C2']);
  });

  it('builds one confirmation per statement, excluded ones included', () => {
    let rows = initialRows(extracted);
    rows = toggleDependency(rows, 'C1', 'C2', true);
    rows = updateRow(rows, 'C4', { excluded: true });
    rows = updateRow(rows, 'C2', { kind: 'proposition' });
    expect(buildConfirmations(rows)).toEqual({
      ok: true,
      value: [
        { id: 'C1', kind: 'theorem', role: 'main', depends_on: ['C2'] },
        { id: 'C2', kind: 'proposition', role: 'supporting', depends_on: [] },
        { id: 'C3', kind: 'conjecture', role: 'supporting', depends_on: [] },
        { id: 'C4', kind: 'claim', role: 'supporting', excluded: true },
      ],
    });
  });

  it('keeps dependencies in paper order', () => {
    let rows = initialRows(extracted);
    rows = toggleDependency(rows, 'C3', 'C2', true);
    rows = toggleDependency(rows, 'C3', 'C1', true);
    expect(rows[2]?.dependsOn).toEqual(['C1', 'C2']);
    rows = toggleDependency(rows, 'C3', 'C1', false);
    expect(rows[2]?.dependsOn).toEqual(['C2']);
  });

  it('refuses what the server refuses', () => {
    const rows = initialRows(extracted);
    // No proved main result: the only main statement becomes a conjecture.
    expect(buildConfirmations(updateRow(rows, 'C1', { kind: 'conjecture' }))).toMatchObject({
      ok: false,
      error: expect.stringMatching(/main result/),
    });
    // A dependency on an excluded statement.
    const excludedDep = updateRow(toggleDependency(rows, 'C1', 'C2', true), 'C2', {
      excluded: true,
    });
    expect(buildConfirmations(excludedDep)).toEqual({
      ok: false,
      error: 'C1 depends on C2, which is excluded.',
    });
    // A cycle.
    const cyclic = toggleDependency(toggleDependency(rows, 'C1', 'C2', true), 'C2', 'C1', true);
    expect(buildConfirmations(cyclic)).toMatchObject({ ok: false, error: /cycle/ });
    // Everything excluded.
    expect(buildConfirmations(rows.map((r) => ({ ...r, excluded: true })))).toEqual({
      ok: false,
      error: 'Keep at least one statement.',
    });
    // A settled open problem needs a name and a source.
    expect(
      buildConfirmations(
        updateRow(rows, 'C1', { settles: { name: '', source: { kind: 'doi', locator: '' } } }),
      ),
    ).toMatchObject({ ok: false });
  });

  it('sends a settled open problem trimmed', () => {
    const rows = updateRow(initialRows(extracted), 'C1', {
      settles: { name: ' Erdős #1 ', source: { kind: 'url', locator: ' https://example.org/1 ' } },
    });
    const built = buildConfirmations(rows);
    expect(built.ok && built.value[0]?.settles).toEqual({
      name: 'Erdős #1',
      source: { kind: 'url', locator: 'https://example.org/1' },
    });
  });
});
