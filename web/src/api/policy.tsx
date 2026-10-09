import { createContext, useCallback, useContext, type ReactNode } from 'react';
import { useApi } from './context';
import type { PolicyDocument } from './types';
import { useAsync, type AsyncState } from './useAsync';

const PolicyContext = createContext<AsyncState<PolicyDocument>>({ status: 'idle' });

/** Loads `GET /policy` once for the whole app; pages fall back to local text without it. */
export function PolicyProvider({ children }: { children: ReactNode }) {
  const api = useApi();
  const load = useCallback((signal: AbortSignal) => api.policy({ signal }), [api]);
  const { state } = useAsync(load);
  return <PolicyContext.Provider value={state}>{children}</PolicyContext.Provider>;
}

export function usePolicyState(): AsyncState<PolicyDocument> {
  return useContext(PolicyContext);
}

export function usePolicy(): PolicyDocument | null {
  const state = useContext(PolicyContext);
  return state.status === 'ok' ? state.value : null;
}
