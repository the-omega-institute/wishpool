import type { TaskKind } from '../api/types';

export type Route =
  | { kind: 'home' }
  | { kind: 'papers' }
  | { kind: 'paper'; record: string }
  | { kind: 'policy' }
  | { kind: 'submit' }
  | { kind: 'submissions' }
  | { kind: 'queue' }
  | { kind: 'submission'; id: string }
  | { kind: 'people' }
  | { kind: 'contribute' }
  | { kind: 'tasks'; submission?: string; taskKind?: TaskKind }
  | { kind: 'task'; id: string }
  | { kind: 'contributors' }
  | { kind: 'not_found'; path: string };

const TASK_KIND_VALUES: readonly string[] = [
  'judge_escape',
  'literature_check',
  'formalize',
  'probe',
];

function decodeSegment(value: string): string | null {
  if (value === '') return null;
  try {
    const decoded = decodeURIComponent(value);
    return decoded === '' || decoded === '.' || decoded === '..' || decoded.includes('/')
      ? null
      : decoded;
  } catch {
    return null;
  }
}

/** Parse a location (`pathname` plus optional `search`) into a route. */
export function parseRoute(pathname: string, search = ''): Route {
  const path = pathname.length > 1 ? pathname.replace(/\/+$/, '') : pathname;
  if (path === '/' || path === '') return { kind: 'home' };
  const notFound: Route = { kind: 'not_found', path: pathname };
  if (!path.startsWith('/')) return notFound;
  const segments = path.slice(1).split('/').map(decodeSegment);
  if (segments.some((s) => s === null)) return notFound;
  const [first, second] = segments as string[];

  if (segments.length === 1) {
    switch (first) {
      case 'papers':
        return { kind: 'papers' };
      case 'policy':
        return { kind: 'policy' };
      case 'submit':
        return { kind: 'submit' };
      case 'submissions':
        return { kind: 'submissions' };
      case 'queue':
        return { kind: 'queue' };
      case 'people':
        return { kind: 'people' };
      case 'contribute':
        return { kind: 'contribute' };
      case 'tasks': {
        const params = new URLSearchParams(search);
        const route: Route = { kind: 'tasks' };
        const submission = params.get('submission');
        const kind = params.get('kind');
        if (submission) route.submission = submission;
        if (kind && TASK_KIND_VALUES.includes(kind)) route.taskKind = kind as TaskKind;
        return route;
      }
      case 'contributors':
        return { kind: 'contributors' };
    }
    return notFound;
  }
  if (segments.length === 2 && second !== undefined) {
    switch (first) {
      case 'papers':
        return { kind: 'paper', record: second };
      case 'submissions':
        return { kind: 'submission', id: second };
      case 'tasks':
        return { kind: 'task', id: second };
    }
  }
  return notFound;
}

const seg = encodeURIComponent;

export function routePath(route: Route): string {
  switch (route.kind) {
    case 'home':
      return '/';
    case 'papers':
      return '/papers';
    case 'paper':
      return `/papers/${seg(route.record)}`;
    case 'policy':
      return '/policy';
    case 'submit':
      return '/submit';
    case 'submissions':
      return '/submissions';
    case 'queue':
      return '/queue';
    case 'submission':
      return `/submissions/${seg(route.id)}`;
    case 'people':
      return '/people';
    case 'contribute':
      return '/contribute';
    case 'tasks': {
      const params = new URLSearchParams();
      if (route.submission) params.set('submission', route.submission);
      if (route.taskKind) params.set('kind', route.taskKind);
      const q = params.toString();
      return q === '' ? '/tasks' : `/tasks?${q}`;
    }
    case 'task':
      return `/tasks/${seg(route.id)}`;
    case 'contributors':
      return '/contributors';
    case 'not_found':
      return route.path;
  }
}
