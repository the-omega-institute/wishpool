import { useCallback, useState } from 'react';
import type { Listing } from './types';
import { useAsync, type AsyncResource } from './useAsync';

export type PageFetcher<T> = (before: string | null, signal: AbortSignal) => Promise<Listing<T>>;

export interface Paged<T> extends AsyncResource<Listing<T>> {
  /** 1-based page number. */
  page: number;
  hasOlder: boolean;
  hasNewer: boolean;
  older: () => void;
  newer: () => void;
}

/**
 * Cursor pagination over a newest-first listing, one page at a time. A new
 * `fetchPage` identity (e.g. changed filters) starts again from the first page.
 */
export function usePaged<T>(fetchPage: PageFetcher<T> | null): Paged<T> {
  const [pager, setPager] = useState<{ key: PageFetcher<T> | null; cursors: (string | null)[] }>({
    key: fetchPage,
    cursors: [],
  });
  const cursors = pager.key === fetchPage ? pager.cursors : [];
  const cursor = cursors.length > 0 ? (cursors[cursors.length - 1] ?? null) : null;

  const load = useCallback(
    (signal: AbortSignal) => (fetchPage as PageFetcher<T>)(cursor, signal),
    [fetchPage, cursor],
  );
  const resource = useAsync(fetchPage === null ? null : load);
  const next = resource.value?.next_before ?? null;

  const older = useCallback(() => {
    if (next === null) return;
    setPager((prev) => ({
      key: fetchPage,
      cursors: [...(prev.key === fetchPage ? prev.cursors : []), next],
    }));
  }, [fetchPage, next]);
  const newer = useCallback(() => {
    setPager((prev) => ({
      key: fetchPage,
      cursors: (prev.key === fetchPage ? prev.cursors : []).slice(0, -1),
    }));
  }, [fetchPage]);

  return {
    ...resource,
    page: cursors.length + 1,
    hasOlder: next !== null,
    hasNewer: cursors.length > 0,
    older,
    newer,
  };
}
