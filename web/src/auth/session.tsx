import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react';
import { currentPath, errorMessage, signIn } from '../api/client';
import { useApi } from '../api/context';
import type { Person, Role } from '../api/types';

export type SessionState =
  | { status: 'loading' }
  | { status: 'anonymous' }
  | { status: 'signed_in'; person: Person; donationsEnabled: boolean }
  | { status: 'error'; message: string };

export interface SessionContextValue {
  session: SessionState;
  person: Person | null;
  /** The server runs local development sign-in (no provider). */
  devSignIn: boolean;
  signIn: (returnTo?: string) => void;
  signOut: () => Promise<void>;
  refresh: () => void;
}

const SessionContext = createContext<SessionContextValue | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  const api = useApi();
  const [session, setSession] = useState<SessionState>({ status: 'loading' });
  const [nonce, setNonce] = useState(0);
  const [devSignIn, setDevSignIn] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    api.session({ signal: controller.signal }).then(
      (s) => {
        if (controller.signal.aborted) return;
        setDevSignIn(s.dev_sign_in === true);
        setSession(
          s.authenticated
            ? {
                status: 'signed_in',
                person: s.person,
                donationsEnabled: s.donations_enabled !== false,
              }
            : { status: 'anonymous' },
        );
      },
      (error: unknown) => {
        if (!controller.signal.aborted)
          setSession({ status: 'error', message: errorMessage(error) });
      },
    );
    return () => controller.abort();
  }, [api, nonce]);

  const signOut = useCallback(async () => {
    await api.logout();
    setSession({ status: 'anonymous' });
  }, [api]);

  const value: SessionContextValue = {
    session,
    person: session.status === 'signed_in' ? session.person : null,
    devSignIn,
    signIn: (returnTo) => signIn(returnTo ?? currentPath()),
    signOut,
    refresh: () => setNonce((n) => n + 1),
  };
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession(): SessionContextValue {
  const value = useContext(SessionContext);
  if (value === null) throw new Error('useSession must be used inside <SessionProvider>');
  return value;
}

export function hasRole(person: Person | null, ...roles: Role[]): boolean {
  return person !== null && roles.some((role) => person.roles.includes(role));
}
