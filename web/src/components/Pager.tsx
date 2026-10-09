import type { Paged } from '../api/usePaged';

export function Pager<T>({
  paged,
  labels = { newer: '← Newer', older: 'Older →' },
}: {
  paged: Paged<T>;
  /** Button text, e.g. `← Previous` / `Next →` for listings not ordered by date. */
  labels?: { newer: string; older: string };
}) {
  if (!paged.hasNewer && !paged.hasOlder) return null;
  return (
    <nav className="pager" aria-label="Pagination">
      <button
        type="button"
        className="button button-quiet"
        disabled={!paged.hasNewer}
        onClick={paged.newer}
      >
        {labels.newer}
      </button>
      <span className="muted small">Page {paged.page}</span>
      <button
        type="button"
        className="button button-quiet"
        disabled={!paged.hasOlder}
        onClick={paged.older}
      >
        {labels.older}
      </button>
    </nav>
  );
}
