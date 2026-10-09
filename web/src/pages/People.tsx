import { useCallback, useState } from 'react';
import { useApi } from '../api/context';
import { usePaged } from '../api/usePaged';
import type { Person, Role } from '../api/types';
import { hasRole, useSession } from '../auth/session';
import { Pager } from '../components/Pager';
import { Async, DateText, InlineError, Loading, SignInPrompt } from '../components/ui';
import { ROLES, ROLE_LABELS } from '../lib/labels';

export function PeoplePage() {
  const api = useApi();
  const { session, person } = useSession();
  const isAdmin = hasRole(person, 'admin');
  const fetchPage = useCallback(
    (before: string | null, signal: AbortSignal) =>
      api.listPeople({ before, limit: 50 }, { signal }),
    [api],
  );
  const paged = usePaged(isAdmin ? fetchPage : null);

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <p className="eyebrow">Admin</p>
          <h1>People</h1>
          <p className="lede">Everyone who has signed in, and the editorial roles they hold.</p>
        </div>
      </header>
      {session.status === 'loading' ? <Loading /> : null}
      {session.status !== 'loading' && person === null ? (
        <SignInPrompt what="manage people" />
      ) : null}
      {person !== null && !isAdmin ? (
        <p className="notice notice-info">Only admins can manage roles.</p>
      ) : null}
      <Async state={paged.state} onRetry={paged.reload}>
        {(listing) =>
          listing.items.length === 0 ? (
            <p className="muted">No one yet.</p>
          ) : (
            <ul className="people">
              {listing.items.map((p) => (
                <PersonRow key={p.id} person={p} isSelf={p.id === person?.id} />
              ))}
            </ul>
          )
        }
      </Async>
      <Pager paged={paged} />
    </div>
  );
}

function sameRoles(a: readonly Role[], b: readonly Role[]): boolean {
  return a.length === b.length && a.every((r) => b.includes(r));
}

function PersonRow({ person: initial, isSelf }: { person: Person; isSelf: boolean }) {
  const api = useApi();
  const [person, setPerson] = useState(initial);
  const [roles, setRoles] = useState<Role[]>(initial.roles);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [saved, setSaved] = useState(false);
  const dirty = !sameRoles(roles, person.roles);

  const toggle = (role: Role, on: boolean) => {
    setSaved(false);
    setRoles((current) =>
      on
        ? ROLES.filter((r) => r === role || current.includes(r))
        : current.filter((r) => r !== role),
    );
  };
  const save = () => {
    setBusy(true);
    setError(null);
    api.setRoles(person.id, roles).then(
      (p) => {
        setBusy(false);
        setPerson(p);
        setRoles(p.roles);
        setSaved(true);
      },
      (e: unknown) => {
        setBusy(false);
        setError(e);
      },
    );
  };

  return (
    <li className="person">
      <div className="person-id">
        <p className="person-name">
          {person.display_name}
          {isSelf ? <span className="muted small"> (you)</span> : null}
        </p>
        <p className="muted small">
          {[person.email, person.affiliation, person.orcid].filter(Boolean).join(' · ')}
        </p>
        <p className="muted small">
          Last seen <DateText iso={person.last_seen_at} />
        </p>
      </div>
      <fieldset className="role-boxes">
        <legend className="visually-hidden">Roles of {person.display_name}</legend>
        {ROLES.map((role) => (
          <label key={role} className="checkbox">
            <input
              type="checkbox"
              checked={roles.includes(role)}
              onChange={(e) => toggle(role, e.target.checked)}
            />
            <span>{ROLE_LABELS[role]}</span>
          </label>
        ))}
      </fieldset>
      <div className="person-actions">
        <button type="button" className="button" disabled={!dirty || busy} onClick={save}>
          {busy ? 'Saving…' : 'Save roles'}
        </button>
        {saved && !dirty ? (
          <span className="form-success small" role="status">
            Saved
          </span>
        ) : null}
        <InlineError error={error} />
      </div>
    </li>
  );
}
