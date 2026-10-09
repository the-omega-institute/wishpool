import { useCallback, useEffect, useState } from 'react';

export type Loader<T> = (signal: AbortSignal) => Promise<T>;

export type AsyncState<T> =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'ok'; value: T }
  | { status: 'error'; error: unknown };

interface Settled<T> {
  load: Loader<T>;
  nonce: number;
  result: { ok: true; value: T } | { ok: false; error: unknown };
}

export interface AsyncResource<T> {
  state: AsyncState<T>;
  /** The last value loaded for this loader, kept while a reload is in flight. */
  value: T | undefined;
  reload: () => void;
  /** Replace the value, e.g. with the entity a mutation returned. */
  replace: (value: T) => void;
}

/**
 * Run `load` and track its result. Pass a loader memoised with `useCallback`;
 * a new loader identity starts a new request and aborts the previous one.
 * `null` leaves the resource idle (e.g. while signed out).
 */
export function useAsync<T>(load: Loader<T> | null): AsyncResource<T> {
  const [nonce, setNonce] = useState(0);
  const [settled, setSettled] = useState<Settled<T> | null>(null);

  useEffect(() => {
    if (load === null) return;
    const controller = new AbortController();
    load(controller.signal).then(
      (value) => {
        if (!controller.signal.aborted) setSettled({ load, nonce, result: { ok: true, value } });
      },
      (error: unknown) => {
        if (!controller.signal.aborted) setSettled({ load, nonce, result: { ok: false, error } });
      },
    );
    return () => controller.abort();
  }, [load, nonce]);

  const reload = useCallback(() => setNonce((n) => n + 1), []);
  const replace = useCallback(
    (value: T) => {
      if (load !== null) setSettled({ load, nonce, result: { ok: true, value } });
    },
    [load, nonce],
  );

  const sameLoader = settled !== null && settled.load === load;
  const value = sameLoader && settled.result.ok ? settled.result.value : undefined;
  let state: AsyncState<T>;
  if (load === null) state = { status: 'idle' };
  else if (!sameLoader || settled.nonce !== nonce) {
    // A reload keeps showing the previous value rather than flashing a spinner.
    state =
      sameLoader && settled.result.ok
        ? { status: 'ok', value: settled.result.value }
        : { status: 'loading' };
  } else if (settled.result.ok) state = { status: 'ok', value: settled.result.value };
  else state = { status: 'error', error: settled.result.error };

  return { state, value, reload, replace };
}
