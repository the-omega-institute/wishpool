import type { ReactNode } from 'react';
import { isApiError, errorMessage, signIn } from '../api/client';
import type { AsyncState } from '../api/useAsync';
import type { Source } from '../api/types';
import { formatDate, formatDateTime } from '../lib/format';
import { sourceHref, sourceLabel } from '../lib/links';

export type BadgeTone = 'neutral' | 'accent' | 'good' | 'warn' | 'bad' | 'muted';

export function Badge({
  tone = 'neutral',
  children,
  title,
}: {
  tone?: BadgeTone;
  children: ReactNode;
  title?: string;
}) {
  return (
    <span className={`badge badge-${tone}`} title={title}>
      {children}
    </span>
  );
}

export function ExternalLink({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a href={href} target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  );
}

/** A source rendered as a link where it resolves, as text where it does not. */
export function SourceLink({ source }: { source: Source }) {
  const href = sourceHref(source);
  const label = sourceLabel(source);
  return href ? <ExternalLink href={href}>{label}</ExternalLink> : <span>{label}</span>;
}

export function DateText({ iso, withTime = false }: { iso: string; withTime?: boolean }) {
  return (
    <time dateTime={iso} title={formatDateTime(iso)}>
      {withTime ? formatDateTime(iso) : formatDate(iso)}
    </time>
  );
}

export function Loading({ label = 'Loading…' }: { label?: string }) {
  return (
    <p className="muted loading" role="status">
      {label}
    </p>
  );
}

/** An error, with a sign-in action when the server says the caller is anonymous. */
export function ErrorNotice({ error, onRetry }: { error: unknown; onRetry?: () => void }) {
  if (isApiError(error) && error.code === 'not_authenticated') {
    return (
      <div className="notice notice-info" role="alert">
        <p>You need to sign in to see this.</p>
        <button type="button" className="button" onClick={() => signIn()}>
          Sign in
        </button>
      </div>
    );
  }
  const heading =
    isApiError(error) && error.code === 'not_found'
      ? 'Not found'
      : isApiError(error) && error.code === 'forbidden'
        ? 'Not permitted'
        : 'Something went wrong';
  return (
    <div className="notice notice-error" role="alert">
      <p>
        <strong>{heading}.</strong> {errorMessage(error)}
      </p>
      {onRetry ? (
        <button type="button" className="button button-quiet" onClick={onRetry}>
          Try again
        </button>
      ) : null}
    </div>
  );
}

/** Render loading / error / value for an `AsyncState`. */
export function Async<T>({
  state,
  onRetry,
  children,
}: {
  state: AsyncState<T>;
  onRetry?: () => void;
  children: (value: T) => ReactNode;
}) {
  switch (state.status) {
    case 'idle':
      return null;
    case 'loading':
      return <Loading />;
    case 'error':
      return <ErrorNotice error={state.error} onRetry={onRetry} />;
    case 'ok':
      return <>{children(state.value)}</>;
  }
}

export function InlineError({ error }: { error: unknown }) {
  if (error === null || error === undefined) return null;
  return (
    <p className="form-error" role="alert">
      {typeof error === 'string' ? error : errorMessage(error)}
    </p>
  );
}

export function Field({
  label,
  hint,
  children,
  htmlFor,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
  htmlFor?: string;
}) {
  return (
    <div className="field">
      <label htmlFor={htmlFor}>{label}</label>
      {children}
      {hint ? <p className="hint">{hint}</p> : null}
    </div>
  );
}

export function MscList({ codes }: { codes: readonly string[] }) {
  if (codes.length === 0) return null;
  return (
    <span className="msc" aria-label="MSC 2020 codes">
      {codes.map((code) => (
        <code key={code}>{code}</code>
      ))}
    </span>
  );
}

export function SignInPrompt({ what }: { what: string }) {
  return (
    <div className="notice notice-info">
      <p>Sign in to {what}.</p>
      <button type="button" className="button" onClick={() => signIn()}>
        Sign in
      </button>
    </div>
  );
}

/** A secondary section, closed until the reader opens it. */
export function Fold({
  id,
  title,
  hint,
  className,
  children,
}: {
  id: string;
  title: string;
  hint?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  return (
    <details className={className ? `fold ${className}` : 'fold'}>
      <summary>
        <h2 id={id}>{title}</h2>
        {hint !== undefined ? <span className="fold-hint">{hint}</span> : null}
      </summary>
      <div className="fold-body">{children}</div>
    </details>
  );
}
