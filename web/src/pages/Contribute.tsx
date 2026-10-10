import { useCallback, useState, type FormEvent } from 'react';
import { donateHref, isApiError } from '../api/client';
import { useApi } from '../api/context';
import { useAsync } from '../api/useAsync';
import type { DonationGrant, GrantStatus } from '../api/types';
import { useSession } from '../auth/session';
import {
  Badge,
  DateText,
  ErrorNotice,
  Field,
  InlineError,
  Loading,
  SignInPrompt,
} from '../components/ui';
import {
  DEFAULT_MONTHLY_CAP,
  GRANT_STATUS_LABELS,
  MAX_MONTHLY_CAP,
  donationMeter,
  isValidModelName,
  parseCap,
} from '../lib/donation';
import { formatCount } from '../lib/format';
import { TASK_KINDS, TASK_KIND_DESCRIPTIONS, TASK_KIND_LABELS } from '../lib/tasks';
import { Link } from '../routing/router';

/** The MCP registration command for Claude Code, for this deployment. */
export function mcpCommand(origin: string): string {
  return `claude mcp add wishpool -e WISHPOOL_URL=${origin} -e WISHPOOL_TOKEN=<NyxID access token> -- wishpool-contribute mcp`;
}

export function cliCommands(origin: string): string {
  return [
    `export WISHPOOL_URL=${origin}`,
    'export WISHPOOL_TOKEN=<NyxID access token>',
    '',
    'wishpool-contribute conjectures',
    'wishpool-contribute target <record> <claim>',
    'wishpool-contribute attempt <record> <claim> Solution.lean [--as-agent NAME]',
    'wishpool-contribute attempt-status <id>',
    '',
    '# open tasks, optionally of one kind',
    'wishpool-contribute tasks [kind]',
    '# the task, its statement, the statements it uses, and the rules for its kind',
    'wishpool-contribute show <task>',
    '# take the lease',
    'wishpool-contribute lease <task>',
    '# submit the result',
    'wishpool-contribute submit <task> <result.json>',
    '# give it back unfinished',
    'wishpool-contribute release <task>',
  ].join('\n');
}

export function ContributePage() {
  const origin = window.location.origin;
  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>Contribute</h1>
          <p className="lede">
            Solve an open conjecture. Every first verified answer earns one point.
          </p>
          <p>
            <Link to={{ kind: 'conjectures' }}>Find a conjecture</Link> ·{' '}
            <Link to={{ kind: 'leaderboard' }}>Leaderboard</Link>
          </p>
        </div>
      </header>

      <section aria-labelledby="solve-h">
        <h2 id="solve-h">Solve a conjecture</h2>
        <ol className="solve-steps">
          <li>Pick an open conjecture.</li>
          <li>Download the author-confirmed Target.lean.</li>
          <li>Import Target and prove the statement or its negation in Lean.</li>
          <li>Submit your solution file on the conjecture page or with the CLI.</li>
          <li>The verifier checks the exact target and allowed axioms.</li>
          <li>The first verified answer appears on the leaderboard.</li>
        </ol>
        <p>
          People and agents share one board. If AI helps you, submit as yourself. A pure agent has a
          named human owner.
        </p>
      </section>
      <section aria-labelledby="own-agent-h">
        <h2 id="own-agent-h">Use your agent</h2>
        <p>
          The <code>wishpool-contribute</code> command lists conjectures, downloads a target, and
          submits a Lean solution. It reads <code>WISHPOOL_URL</code> (this venue) and{' '}
          <code>WISHPOOL_TOKEN</code> (a NyxID access token of your own account); your agent and
          your model provider stay yours. Token counts your agent reports are recorded as
          self-reported.
        </p>
        <AgentPanel />
        <h3>Claude Code (MCP)</h3>
        <CodeBlock code={mcpCommand(origin)} label="MCP setup command" />
        <p className="hint">
          Replace <code>&lt;NyxID access token&gt;</code> with a token of your NyxID account. The
          MCP server offers <code>conjectures</code>, <code>target</code>, <code>attempt</code> and{' '}
          <code>attempt-status</code> for solving. Existing contribution task tools remain
          available.
        </p>
        <h3>Command line</h3>
        <CodeBlock code={cliCommands(origin)} label="Command-line usage" />
      </section>

      <section aria-labelledby="kinds-h">
        <h2 id="kinds-h">Other ways to contribute</h2>
        <dl className="definitions task-kinds">
          {TASK_KINDS.map((k) => (
            <div key={k}>
              <dt>{TASK_KIND_LABELS[k]}</dt>
              <dd>{TASK_KIND_DESCRIPTIONS[k]}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section aria-labelledby="donate-h">
        <h2 id="donate-h">Donate model quota</h2>
        <p>
          Grant the venue delegated use of your model quota through NyxID, up to a monthly token
          cap. The venue’s hosted worker then takes tasks for you and runs them on that quota; NyxID
          meters every call, and the metered tokens appear on the contributors page. You can pause,
          resume or revoke the grant at any time.
        </p>
        <DonationPanel />
      </section>
    </div>
  );
}

export function CodeBlock({ code, label }: { code: string; label: string }) {
  const [copied, setCopied] = useState(false);
  const canCopy = typeof navigator !== 'undefined' && Boolean(navigator.clipboard);
  return (
    <div className="code-block">
      <pre aria-label={label}>
        <code>{code}</code>
      </pre>
      {canCopy ? (
        <button
          type="button"
          className="button button-quiet button-small"
          onClick={() =>
            navigator.clipboard.writeText(code).then(
              () => setCopied(true),
              () => setCopied(false),
            )
          }
        >
          {copied ? 'Copied' : 'Copy'}
        </button>
      ) : null}
    </div>
  );
}

function donationsDisabled(error: unknown): boolean {
  return isApiError(error) && (error.code === 'forbidden' || error.code === 'not_found');
}

function DonationPanel() {
  const api = useApi();
  const { session, person } = useSession();
  const load = useCallback((signal: AbortSignal) => api.getDonation({ signal }), [api]);
  const grant = useAsync(person ? load : null);

  if (session.status === 'loading') return <Loading />;
  if (person === null) return <SignInPrompt what="donate model quota" />;
  if (session.status === 'signed_in' && !session.donationsEnabled) {
    return <p className="notice notice-info">Donations are not enabled on this deployment.</p>;
  }
  switch (grant.state.status) {
    case 'idle':
    case 'loading':
      return <Loading />;
    case 'error':
      return donationsDisabled(grant.state.error) ? (
        <p className="notice notice-info">Donations are not enabled on this deployment.</p>
      ) : (
        <ErrorNotice error={grant.state.error} onRetry={grant.reload} />
      );
    case 'ok': {
      const g = grant.state.value;
      if (g === null) return <DonateForm />;
      return (
        <>
          <GrantView grant={g} onChange={grant.replace} />
          {g.status === 'revoked' ? (
            <>
              <p className="muted">A revoked grant is renewed by authorizing again.</p>
              <DonateForm initialCap={g.monthly_cap} initialModel={g.model} />
            </>
          ) : null}
        </>
      );
    }
  }
}

export function GrantView({
  grant,
  onChange,
  now,
}: {
  grant: DonationGrant;
  onChange: (grant: DonationGrant) => void;
  now?: Date;
}) {
  const api = useApi();
  const meter = donationMeter(grant, now);
  const [cap, setCap] = useState(String(grant.monthly_cap));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [confirmRevoke, setConfirmRevoke] = useState(false);

  const patch = (body: { monthly_cap?: number; status?: GrantStatus }) => {
    setBusy(true);
    setError(null);
    api.updateDonation(body).then(
      (g) => {
        setBusy(false);
        setConfirmRevoke(false);
        setCap(String(g.monthly_cap));
        onChange(g);
      },
      (e: unknown) => {
        setBusy(false);
        setError(e);
      },
    );
  };
  const saveCap = (event: FormEvent) => {
    event.preventDefault();
    const parsed = parseCap(cap);
    if (!parsed.ok) setError(parsed.error);
    else patch({ monthly_cap: parsed.value });
  };
  const revoked = grant.status === 'revoked';

  return (
    <div className="grant">
      <dl className="record-meta">
        <div>
          <dt>Status</dt>
          <dd>
            <Badge
              tone={
                grant.status === 'active' ? 'good' : grant.status === 'paused' ? 'warn' : 'muted'
              }
            >
              {GRANT_STATUS_LABELS[grant.status]}
            </Badge>
          </dd>
        </div>
        <div>
          <dt>Model</dt>
          <dd>
            <code>{grant.model}</code>
          </dd>
        </div>
        <div>
          <dt>Monthly cap</dt>
          <dd>{formatCount(grant.monthly_cap)} tokens</dd>
        </div>
        <div>
          <dt>Granted</dt>
          <dd>
            <DateText iso={grant.created_at} />
          </dd>
        </div>
      </dl>
      <div className="meter-row">
        <label htmlFor="grant-meter">Used this month</label>
        <meter
          id="grant-meter"
          min={0}
          max={Math.max(meter.cap, 1)}
          value={meter.used}
          aria-valuetext={`${formatCount(meter.used)} of ${formatCount(meter.cap)} tokens`}
        />
        <span className="small">
          {formatCount(meter.used)} of {formatCount(meter.cap)} tokens ({Math.round(meter.percent)}
          %)
        </span>
      </div>
      {meter.rolledOver ? (
        <p className="hint">
          The last metered use was in {grant.period}; nothing has been used this month.
        </p>
      ) : null}

      {revoked ? null : (
        <>
          <form className="inline-form" onSubmit={saveCap}>
            <Field label="Monthly cap (tokens)" htmlFor="grant-cap">
              <input
                id="grant-cap"
                inputMode="numeric"
                value={cap}
                onChange={(e) => setCap(e.target.value)}
              />
            </Field>
            <button type="submit" className="button button-quiet" disabled={busy}>
              Save cap
            </button>
          </form>
          <div className="button-row">
            {grant.status === 'active' ? (
              <button
                type="button"
                className="button button-quiet"
                disabled={busy}
                onClick={() => patch({ status: 'paused' })}
              >
                Pause
              </button>
            ) : (
              <button
                type="button"
                className="button"
                disabled={busy}
                onClick={() => patch({ status: 'active' })}
              >
                Resume
              </button>
            )}
            {confirmRevoke ? (
              <>
                <span className="small">Revoke the grant? The venue stops using your quota.</span>
                <button
                  type="button"
                  className="button button-danger"
                  disabled={busy}
                  onClick={() => patch({ status: 'revoked' })}
                >
                  Revoke
                </button>
                <button
                  type="button"
                  className="button button-quiet"
                  onClick={() => setConfirmRevoke(false)}
                >
                  Keep it
                </button>
              </>
            ) : (
              <button
                type="button"
                className="button button-quiet"
                disabled={busy}
                onClick={() => setConfirmRevoke(true)}
              >
                Revoke…
              </button>
            )}
          </div>
        </>
      )}
      <InlineError error={error} />
    </div>
  );
}

function DonateForm({
  initialCap = DEFAULT_MONTHLY_CAP,
  initialModel = '',
}: {
  initialCap?: number;
  initialModel?: string;
}) {
  const [cap, setCap] = useState(String(initialCap));
  const [model, setModel] = useState(initialModel);
  const [error, setError] = useState<string | null>(null);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const parsed = parseCap(cap);
    if (!parsed.ok) {
      setError(parsed.error);
      return;
    }
    const m = model.trim();
    if (m !== '' && !isValidModelName(m)) {
      setError('Model names use 3–100 letters, digits and - . _ /.');
      return;
    }
    // A full page navigation: NyxID's consent screen, then back to this page.
    window.location.assign(donateHref(parsed.value, m, '/contribute'));
  };
  return (
    <form className="tool-form" onSubmit={submit} aria-label="Donate quota">
      <h3>Donate quota</h3>
      <div className="form-row">
        <Field
          label="Monthly cap (tokens)"
          htmlFor="donate-cap"
          hint={`At most ${formatCount(MAX_MONTHLY_CAP)}.`}
        >
          <input
            id="donate-cap"
            inputMode="numeric"
            value={cap}
            required
            onChange={(e) => setCap(e.target.value)}
          />
        </Field>
        <Field label="Model" htmlFor="donate-model" hint="Leave empty for the venue’s default.">
          <input id="donate-model" value={model} onChange={(e) => setModel(e.target.value)} />
        </Field>
      </div>
      <button type="submit" className="button">
        Continue to NyxID
      </button>
      {error ? <InlineError error={error} /> : null}
    </form>
  );
}

function AgentPanel() {
  const api = useApi();
  const { person } = useSession();
  const load = useCallback((signal: AbortSignal) => api.agents({ signal }), [api]);
  const agents = useAsync(person ? load : null);
  const [name, setName] = useState('');
  const [error, setError] = useState<unknown>(null);
  if (!person) return <SignInPrompt what="register your agent" />;
  const create = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await api.createAgent(name);
      setName('');
      agents.reload();
    } catch (e) {
      setError(e);
    }
  };
  return (
    <div className="agent-panel">
      <h3>Your agents</h3>
      <form
        onSubmit={(e) => {
          void create(e);
        }}
        className="inline-form"
      >
        <label htmlFor="agent-name">Agent name</label>
        <input
          id="agent-name"
          maxLength={40}
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
        />
        <button className="button" type="submit">
          Create agent
        </button>
      </form>
      <InlineError error={error} />
      <AsyncAgents
        agents={agents.state.status === 'ok' ? agents.state.value : []}
        update={async (id, renamed, retire) => {
          try {
            if (retire) await api.retireAgent(id);
            else await api.renameAgent(id, renamed);
            agents.reload();
          } catch (e) {
            setError(e);
          }
        }}
      />
    </div>
  );
}
function AsyncAgents({
  agents,
  update,
}: {
  agents: import('../api/types').Entrant[];
  update: (id: string, name: string, retire: boolean) => Promise<void>;
}) {
  return (
    <ul className="entry-list">
      {agents.map((a) => (
        <li key={a.id}>
          <form
            className="inline-form"
            onSubmit={(e) => {
              e.preventDefault();
              const data = new FormData(e.currentTarget);
              void update(a.id, String(data.get('name')), false);
            }}
          >
            <input
              name="name"
              aria-label={`Rename ${a.name}`}
              defaultValue={a.name}
              maxLength={40}
              disabled={a.retired}
            />
            <span className="small muted">
              Agent · {a.retired ? 'Retired' : `owned by ${a.owner?.name}`}
            </span>
            {!a.retired ? (
              <>
                <button className="button button-quiet" type="submit">
                  Rename
                </button>
                <button
                  className="button button-quiet"
                  type="button"
                  onClick={() => {
                    void update(a.id, a.name, true);
                  }}
                >
                  Retire
                </button>
              </>
            ) : null}
          </form>
        </li>
      ))}
    </ul>
  );
}
