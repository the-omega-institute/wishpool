import { describe, expect, it } from 'vitest';
import { parseRoute, routePath, type Route } from './routes';

const ROUTES: Route[] = [
  { kind: 'home' },
  { kind: 'papers' },
  { kind: 'paper', record: 'WP-2026-0001' },
  { kind: 'policy' },
  { kind: 'submit' },
  { kind: 'submissions' },
  { kind: 'queue' },
  { kind: 'submission', id: 'sub-1' },
  { kind: 'people' },
  { kind: 'contribute' },
  { kind: 'tasks' },
  { kind: 'tasks', submission: 'sub 1' },
  { kind: 'tasks', submission: 'sub-1', taskKind: 'literature_check' },
  { kind: 'task', id: 'task-1' },
  { kind: 'contributors' },
];

describe('routes', () => {
  it.each(ROUTES)('round-trips %o', (route) => {
    const path = routePath(route);
    const [pathname, search = ''] = path.split('?');
    expect(parseRoute(pathname ?? '', search ? `?${search}` : '')).toEqual(route);
  });

  it('tolerates a trailing slash', () => {
    expect(parseRoute('/papers/')).toEqual({ kind: 'papers' });
  });

  it('ignores an unknown task kind in the query', () => {
    expect(parseRoute('/tasks', '?kind=wish')).toEqual({ kind: 'tasks' });
  });

  it.each([
    '/nope',
    '/papers/a/b',
    '/papers/%',
    '/submissions/..',
    '/wishes',
    '/ledger',
    '/records',
  ])('rejects %s', (p) => {
    expect(parseRoute(p)).toEqual({ kind: 'not_found', path: p });
  });
});
