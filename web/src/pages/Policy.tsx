import { usePolicyState } from '../api/policy';
import { Async } from '../components/ui';
import { BASIS_DESCRIPTIONS, BASIS_LABELS, rejectReasonText } from '../lib/labels';
import { STAGES, THRESHOLD_TEXT, stageInfo, stageName } from '../lib/stages';
import type { AdmissionBasis, RejectReason } from '../api/types';

const BASES: AdmissionBasis[] = ['escape_witness', 'open_problem_settlement', 'open_conjecture'];

const REASONS: RejectReason[] = [
  { reason: 'hygiene', detail: 'the source does not compile or AI use is not disclosed' },
  { reason: 'known_result', claim: 'C1', prior: { kind: 'doi', locator: '10.…' } },
  { reason: 'bind_only' },
  { reason: 'no_main_result' },
  {
    reason: 'conjecture',
    detail:
      'The conjecture needs a well-posed, open statement whose proof would carry new mathematical content.',
  },
  { reason: 'out_of_scope', detail: 'the paper is not a mathematics paper' },
];

export function PolicyPage() {
  const state = usePolicyState();
  return (
    <div className="page narrow-page">
      <header className="page-head">
        <div>
          <h1>Review policy</h1>
          <p className="lede">
            Papers, short notes and conjectures use the same four review stages. The decision is a
            published function of the stage reports; the same function gives the author a preview
            while the work is in review.
          </p>
        </div>
      </header>
      <Async state={state.status === 'idle' ? { status: 'loading' } : state}>
        {(policy) => (
          <>
            <section aria-labelledby="stages-h">
              <h2 id="stages-h">Stages</h2>
              <dl className="definitions">
                {STAGES.map((stage) => {
                  const info = stageInfo(stage, policy);
                  const human = policy.policy.human_judgement.includes(stage);
                  return (
                    <div key={stage}>
                      <dt>{stageName(stage, policy)}</dt>
                      <dd>{info.description}</dd>
                      <dd className="muted small">
                        {stage === 'claims'
                          ? 'Filed by the author’s confirmation.'
                          : human
                            ? 'Decided by an editor; machine reports on this stage are proposals.'
                            : 'Filed automatically.'}
                      </dd>
                    </div>
                  );
                })}
              </dl>
              <p className="small">
                An author may have at most {policy.policy.max_active_per_author} submissions in
                draft or review at once.
              </p>
            </section>
          </>
        )}
      </Async>

      <section aria-labelledby="threshold-h">
        <h2 id="threshold-h">Threshold</h2>
        <p>{THRESHOLD_TEXT} Short notes use the same threshold.</p>
        <p>
          A conjecture is displayed when at least one main statement is audited as well posed, open,
          and mathematically new if proved.
        </p>
        <dl className="definitions">
          {BASES.map((b) => (
            <div key={b}>
              <dt>{BASIS_LABELS[b]}</dt>
              <dd>{BASIS_DESCRIPTIONS[b]}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section aria-labelledby="escape-h">
        <h2 id="escape-h">Escape analysis</h2>
        <p>
          A proved statement is <strong>bind-only</strong> when its proof obtains it from prior
          results by instantiation, projection or normalisation. It carries <strong>content</strong>{' '}
          when its proof path contains escape witnesses: propositions that prior results do not give
          by binding alone. Judgements of each statement are combined: an editor’s judgement
          confirms; two machine judgements from different model families and accounts that agree
          corroborate. Editors file S3 from the settled judgements.
        </p>
      </section>

      <section aria-labelledby="reasons-h">
        <h2 id="reasons-h">Reasons a paper is not accepted</h2>
        <ul>
          {REASONS.map((r) => (
            <li key={r.reason}>{rejectReasonText(r)}</li>
          ))}
        </ul>
        <p>
          The report and its reasons stay private to the authors. The author may upload a revised
          version; it returns to draft and the statements are confirmed again.
        </p>
      </section>

      <section aria-labelledby="visibility-h">
        <h2 id="visibility-h">Visibility</h2>
        <ul>
          <li>
            A paper in draft, in review, not accepted or withdrawn is visible to its authors and the
            editors only.
          </li>
          <li>
            When the author allows volunteer contributors, signed-in contributors can read each
            statement, its dependencies, and the paper’s title and abstract while the work is in
            review.
          </li>
          <li>
            “Make public after acceptance” is checked by default for every kind. The public page
            shows statements, dependencies, new intermediate lemmas and verified Lean proofs.
            Authors can change the setting later; private records show only title, authors, kind and
            record id. Reviews, correctness labels, comments, reports, advice and letters stay
            private.
          </li>
        </ul>
        <p>
          Accepted conjectures receive an elaborated Lean statement after the letter. The author
          confirms its exact text or asks for a correction. This checks the statement’s meaning and
          well-formedness; it does not verify a proof. Attack attempts follow in Phase B.
        </p>
      </section>
    </div>
  );
}
