import { useCallback, useEffect, useState } from 'react';
import { pdfHref, sourceFileHref } from '../api/client';
import { useApi } from '../api/context';
import { usePolicy } from '../api/policy';
import { useAsync } from '../api/useAsync';
import { useReferee } from '../api/useReferee';
import type { Decision, PolicyDocument, RejectReason, Submission } from '../api/types';
import { hasRole, useSession } from '../auth/session';
import { AnalysisView, ConjectureList } from '../components/Analysis';
import { AuthorLine } from '../components/AuthorLine';
import { BasisBadge, SubmissionStatusBadge } from '../components/badges';
import { StatementList } from '../components/claims';
import { LatexText, MacrosProvider } from '../components/Markdown';
import { StageTimeline } from '../components/StageTimeline';
import { StatementSummary } from '../components/StatementSummary';
import { PaperStanding } from '../components/PaperStanding';
import { RefereeWorkspace } from '../components/referee';
import { Async, Badge, DateText, ExternalLink, Fold, Loading } from '../components/ui';
import {
  ContributorsToggle,
  FormalizationForAuthor,
  NewVersionForm,
  VisibilityChoice,
  LeanStatementCard,
  WithdrawPanel,
} from '../components/workspace/AuthorPanels';
import { ConfirmStatements } from '../components/workspace/ConfirmStatements';
import { EditorTools } from '../components/workspace/EditorTools';
import {
  AI_USE_LABELS,
  decisionHeadline,
  formatBytes,
  rejectReasonText,
  SUBMISSION_KIND_LABELS,
} from '../lib/labels';
import { sourceHref } from '../lib/links';
import { currentMacros } from '../lib/macros';
import { stageName } from '../lib/stages';
import { Link } from '../routing/router';

export function SubmissionDetailPage({ id }: { id: string }) {
  const api = useApi();
  const { session } = useSession();
  const load = useCallback((signal: AbortSignal) => api.getSubmission(id, { signal }), [api, id]);
  const submission = useAsync(session.status === 'loading' ? null : load);
  return (
    <div className="page">
      {session.status === 'loading' ? <Loading /> : null}
      <Async state={submission.state} onRetry={submission.reload}>
        {(s) => <Workspace submission={s} onChange={submission.replace} />}
      </Async>
    </div>
  );
}

function isAuthorOf(s: Submission, personId: string | undefined): boolean {
  if (personId === undefined) return false;
  return s.submitter === personId || s.authors.some((a) => a.person === personId);
}

export function Workspace({
  submission: s,
  onChange,
}: {
  submission: Submission;
  onChange: (s: Submission) => void;
}) {
  const api = useApi();
  const policy = usePolicy();
  const { person } = useSession();
  const isAuthor = isAuthorOf(s, person?.id);
  const isEditor = hasRole(person, 'editor', 'admin');
  const isStaff = hasRole(person, 'editor', 'reviewer', 'admin');
  const state = s.status.state;
  const reviewed = state !== 'draft' && s.claims.length > 0;
  const referee = useReferee(s, isStaff, isStaff || isAuthor);
  const reviewRevision = referee.value?.revision;
  useEffect(() => {
    if (reviewRevision === undefined) return;
    const controller = new AbortController();
    void api
      .getSubmission(s.id, { signal: controller.signal })
      .then((current) => {
        if (!controller.signal.aborted && current.revision > s.revision) onChange(current);
      })
      .catch(() => {
        /* The existing paper and referee fetches handle visible errors. */
      });
    return () => controller.abort();
  }, [api, s.id, s.revision, reviewRevision, onChange]);
  useEffect(() => {
    if (s.kind !== 'conjecture' || s.status.state !== 'accepted' || !isAuthor) return;
    const controller = new AbortController();
    const refresh = () => {
      void api
        .getSubmission(s.id, { signal: controller.signal })
        .then((updated) => {
          if (!controller.signal.aborted && updated.revision > s.revision) onChange(updated);
        })
        .catch(() => {
          /* The existing view remains available during a temporary failure. */
        });
    };
    const timer = window.setInterval(refresh, 30_000);
    window.addEventListener('focus', refresh);
    return () => {
      controller.abort();
      window.clearInterval(timer);
      window.removeEventListener('focus', refresh);
    };
  }, [api, s.id, s.kind, s.status.state, s.revision, isAuthor, onChange]);
  const [revising, setRevising] = useState(false);
  const showRevisionForm =
    revising &&
    (state === 'in_review' || state === 'accepted' || state === 'not_accepted') &&
    isAuthor;

  // Keyed on the revision, so every change reloads the preview and the analysis.
  const revision = s.revision;
  const loadDecision = useCallback(
    (signal: AbortSignal) => {
      void revision;
      return api.previewDecision(s.id, { signal });
    },
    [api, s.id, revision],
  );
  const decision = useAsync(state === 'withdrawn' ? null : loadDecision);
  const loadAnalysis = useCallback(
    (signal: AbortSignal) => {
      void revision;
      return api.getAnalysis(s.id, { signal });
    },
    [api, s.id, revision],
  );
  const analysis = useAsync(reviewed ? loadAnalysis : null);
  const preview = decision.value ?? s.decision ?? null;
  const current = s.versions[s.versions.length - 1];
  const currentReview = [...(referee.value?.rounds ?? [])]
    .reverse()
    .find((r) => r.version === current?.number && r.claims_revision === s.claims_revision);

  return (
    <MacrosProvider macros={currentMacros(s.versions)}>
      <article className="workspace">
        <header className="record-head">
          <h1>{s.title || 'Untitled paper'}</h1>
          <AuthorLine authors={s.authors} />
          <p className="entry-meta">
            <span>{SUBMISSION_KIND_LABELS[s.kind]}</span>
            <SubmissionStatusBadge status={s.status} />
            <span>
              Version {current?.number ?? 1} · <DateText iso={s.created_at} />
            </span>
            {s.doi ? <DoiLink doi={s.doi} /> : null}
          </p>
          <div className="button-row">
            {current?.pdf ? (
              <a
                className="button"
                href={pdfHref(s.id, current.number)}
                target="_blank"
                rel="noopener noreferrer"
              >
                View PDF
              </a>
            ) : null}
            {current ? (
              <a className="button button-quiet" href={sourceFileHref(s.id, current.number)}>
                Download source
              </a>
            ) : null}
            {s.status.state === 'accepted' ? (
              <Link className="button button-quiet" to={{ kind: 'paper', record: s.status.record }}>
                Public page {s.status.record}
              </Link>
            ) : null}
          </div>
        </header>

        {isAuthor || isStaff ? (
          <PaperStanding
            submission={s}
            round={currentReview}
            onRevise={isAuthor && !showRevisionForm ? () => setRevising(true) : undefined}
          />
        ) : null}
        {showRevisionForm ? (
          <div id="standing-revision-form">
            <NewVersionForm
              submission={s}
              onChange={(updated) => {
                setRevising(false);
                onChange(updated);
              }}
            />
          </div>
        ) : null}

        {state === 'accepted' &&
        (s.kind === 'conjecture' || s.lean_statements.length > 0) &&
        isAuthor ? (
          <LeanStatementCard submission={s} onChange={onChange} />
        ) : null}
        {reviewed && (isAuthor || isStaff) ? (
          <StatementSummary submission={s} round={currentReview} />
        ) : null}
        {isStaff || isAuthor ? (
          <RefereeWorkspace
            key={`${s.id}-${isStaff}`}
            submission={s}
            isStaff={isStaff}
            isEditor={isEditor}
            resource={referee}
            onChange={onChange}
          />
        ) : null}

        {state === 'draft' ? <DraftNotice submission={s} /> : null}
        {state === 'draft' && isAuthor ? (
          <section aria-labelledby="confirm-h">
            <h2 id="confirm-h">Confirm the statements</h2>
            <ConfirmStatements
              key={`${s.id}-${s.versions.length}`}
              submission={s}
              onChange={onChange}
            />
          </section>
        ) : null}

        {state === 'accepted' && isAuthor ? (
          <section aria-labelledby="accepted-h">
            <h2 id="accepted-h">After acceptance</h2>
            <p>
              Record <code>{s.status.state === 'accepted' ? s.status.record : ''}</code>
              {s.decision?.decision === 'accept' ? (
                <>
                  {' '}
                  · <BasisBadge basis={s.decision.basis} />
                </>
              ) : null}
            </p>
            <VisibilityChoice submission={s} onChange={onChange} />
            {s.kind !== 'conjecture' ? (
              <>
                <h3>Formalization in Lean</h3>
                <FormalizationForAuthor submission={s} onChange={onChange} />
              </>
            ) : null}
          </section>
        ) : null}

        <div className="folds">
          {reviewed && isStaff ? (
            <Fold id="statements-h" title="Statements" hint={s.claims.length}>
              <StatementList claims={s.claims} scope="ws" />
            </Fold>
          ) : null}

          {reviewed && isStaff ? (
            <Fold id="analysis-h" title="Analysis">
              <Async state={analysis.state} onRetry={analysis.reload}>
                {(a) => <AnalysisView analysis={a} claims={s.claims} scope="ws" />}
              </Async>
              {state === 'accepted' ? (
                <>
                  <h3>Conjecture follow-ups</h3>
                  <ConjectureList conjectures={s.conjectures} claims={s.claims} scope="ws" />
                </>
              ) : null}
            </Fold>
          ) : null}

          {isStaff ? (
            <Fold id="stages-h" title="Publication decision">
              {state === 'in_review' && isStaff && preview ? (
                <DecisionView decision={preview} policy={policy} />
              ) : null}
              <StageTimeline submission={s} policy={policy} decision={preview} />
            </Fold>
          ) : null}

          <Fold id="abstract-h" title="Abstract">
            {s.abstract_text ? (
              <LatexText source={s.abstract_text} className="abstract" />
            ) : (
              <p className="muted">No abstract was found in the source.</p>
            )}
            <p className="small">
              <strong>AI disclosure:</strong> {AI_USE_LABELS[s.ai_disclosure.level]} —{' '}
              {s.ai_disclosure.statement}
            </p>
          </Fold>

          <VersionsSection submission={s} />

          {isAuthor ? (
            <Fold id="settings-h" title="Settings">
              {state !== 'withdrawn' && state !== 'not_accepted' ? (
                <ContributorsToggle submission={s} onChange={onChange} />
              ) : null}
              {state === 'draft' || (state === 'in_review' && !showRevisionForm) ? (
                <NewVersionForm submission={s} onChange={onChange} />
              ) : null}
              {state === 'draft' || state === 'in_review' ? (
                <WithdrawPanel submission={s} onChange={onChange} />
              ) : null}
            </Fold>
          ) : null}

          {isEditor ? (
            <EditorTools submission={s} decision={preview} person={person} onChange={onChange} />
          ) : null}
        </div>
      </article>
    </MacrosProvider>
  );
}

function DoiLink({ doi }: { doi: string }) {
  const href = sourceHref({ kind: 'doi', locator: doi });
  return href ? <ExternalLink href={href}>doi:{doi}</ExternalLink> : <span>doi:{doi}</span>;
}

function DraftNotice({ submission }: { submission: Submission }) {
  const current = submission.versions[submission.versions.length - 1];
  if (!current) return null;
  const warnings = current.parse_warnings;
  if (!current.compile_error && warnings.length === 0 && current.pdf) return null;
  return (
    <section aria-label="Reading the source" className="notices">
      {current.compile_error ? (
        <div className="notice notice-error" role="alert">
          <p>
            <strong>The PDF did not compile.</strong>
          </p>
          <pre className="compile-log">{current.compile_error}</pre>
        </div>
      ) : current.pdf ? null : (
        <p className="notice notice-info">The PDF is being compiled.</p>
      )}
      {warnings.length > 0 ? (
        <div className="notice notice-info">
          <p>
            <strong>
              Reading the source raised {warnings.length} warning{warnings.length === 1 ? '' : 's'}:
            </strong>
          </p>
          <ul>
            {warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </div>
      ) : null}
    </section>
  );
}

function DecisionView({ decision, policy }: { decision: Decision; policy: PolicyDocument | null }) {
  const headline = decisionHeadline(decision, (stage) => stageName(stage, policy));
  const tone =
    decision.decision === 'accept'
      ? 'decision-accept'
      : decision.decision === 'not_accepted'
        ? 'decision-reject'
        : 'decision-pending';
  return (
    <div className={`decision ${tone}`}>
      <p className="decision-head">
        <strong>{headline}</strong>
      </p>
      {decision.decision === 'pending' ? <p className="small">{decision.detail}</p> : null}
      {decision.decision === 'accept' ? (
        <p className="small">
          <BasisBadge basis={decision.basis} />
        </p>
      ) : null}
      {decision.decision === 'not_accepted' ? <ReasonList reasons={decision.reasons} /> : null}
    </div>
  );
}

export function ReasonList({ reasons }: { reasons: readonly RejectReason[] }) {
  return (
    <ul className="reason-list">
      {reasons.map((r, i) => (
        <li key={i}>{rejectReasonText(r)}</li>
      ))}
    </ul>
  );
}

function VersionsSection({ submission }: { submission: Submission }) {
  return (
    <Fold id="versions-h" title="Versions" hint={submission.versions.length}>
      <ul className="version-list">
        {[...submission.versions].reverse().map((v) => (
          <li key={v.number}>
            <strong>Version {v.number}</strong> · <DateText iso={v.uploaded_at} withTime /> ·{' '}
            <code>{v.filename}</code> ({formatBytes(v.archive.bytes)}), main file{' '}
            <code>{v.main_file}</code>
            {v.pdf ? (
              <>
                {' '}
                · <a href={pdfHref(submission.id, v.number)}>PDF</a>
              </>
            ) : v.compile_error ? (
              <>
                {' '}
                <Badge tone="bad">PDF failed</Badge>
              </>
            ) : null}{' '}
            · <a href={sourceFileHref(submission.id, v.number)}>source</a>
            {v.note ? <span className="muted"> — {v.note}</span> : null}
          </li>
        ))}
      </ul>
    </Fold>
  );
}
