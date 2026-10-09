import type {
  ConjectureState,
  FileReport,
  FormalVerification,
  NewJudgement,
  PriorWork,
  ProofShape,
  SettlementOutcome,
} from '../api/types';
import { nonStandardAxioms } from './axioms';
import { safeHttpUrl } from './links';
import { fail, lines, ok, type Parsed } from './parsed';

export type LiteratureOutcome = 'pass' | 'fail' | 'needs_human';

export interface LiteratureFields {
  outcome: LiteratureOutcome;
  /** For `fail`: the index into `prior` of the work that states or implies a main result. */
  knownBy: number;
  question: string;
  summary: string;
  searched: string;
  prior: PriorWork[];
}

/** An S2 report filed by an editor (`POST /stages/literature/reports`). */
export function buildLiteratureReport(f: LiteratureFields): Parsed<FileReport> {
  const searched = lines(f.searched);
  if (searched.length === 0) return fail('List where the literature was searched.');
  if (f.summary.trim() === '') return fail('Write a summary of the literature check.');
  const bad = f.prior.findIndex((p) => p.claim === '' || p.source.locator.trim() === '');
  if (bad >= 0) return fail(`Prior work ${bad + 1} needs a statement and a locator.`);
  const prior = f.prior.map((p) => ({ ...p, note: p.note.trim() }));
  const payload = { stage: 'literature' as const, prior, searched };
  switch (f.outcome) {
    case 'pass':
      return ok({ report: { outcome: { outcome: 'pass' }, summary: f.summary.trim(), payload } });
    case 'needs_human':
      return f.question.trim() === ''
        ? fail('State the question for an editor.')
        : ok({
            report: {
              outcome: { outcome: 'needs_human', question: f.question.trim() },
              summary: f.summary.trim(),
              payload,
            },
          });
    case 'fail': {
      const known = prior[f.knownBy];
      if (!known || known.relation === 'related') {
        return fail(
          'A failing literature report names the prior work that states or directly implies a main result.',
        );
      }
      return ok({
        report: {
          outcome: {
            outcome: 'fail',
            reason: { reason: 'known_result', claim: known.claim, prior: known.source },
          },
          summary: f.summary.trim(),
          payload,
        },
      });
    }
  }
}

/** `POST /claims/{claim}/judgements`. */
export function buildJudgement(
  shape: ProofShape,
  witnessText: string,
  rationale: string,
): Parsed<NewJudgement> {
  const witnesses = lines(witnessText);
  if (rationale.trim() === '') return fail('Give the rationale.');
  if (shape === 'content' && witnesses.length === 0) {
    return fail('A content judgement names at least one escape witness.');
  }
  return ok({
    shape,
    witnesses: shape === 'content' ? witnesses : [],
    rationale: rationale.trim(),
  });
}

const HEX40 = /^[0-9a-f]{40}$/i;

/** `POST /formalization/items/{claim}/verification`. */
export function buildVerification(fields: {
  repository: string;
  commit: string;
  declarations: string;
  axioms: string;
  contribution: string;
}): Parsed<FormalVerification> {
  if (safeHttpUrl(fields.repository) === null) return fail('Give the repository URL.');
  const commit = fields.commit.trim();
  if (!HEX40.test(commit)) return fail('The commit is a full 40-character hexadecimal SHA.');
  const declarations = fields.declarations
    .split(/[\s,]+/)
    .map((d) => d.trim())
    .filter((d) => d !== '');
  if (declarations.length === 0) return fail('Name at least one Lean declaration.');
  const axioms = fields.axioms
    .split(/[\s,]+/)
    .map((a) => a.trim())
    .filter((a) => a !== '');
  const flagged = nonStandardAxioms(axioms);
  if (flagged.length > 0) {
    return fail(`Only standard axioms are accepted; found ${flagged.join(', ')}.`);
  }
  const v: FormalVerification = {
    artifact: { repository: fields.repository.trim(), commit: commit.toLowerCase(), declarations },
    axioms,
  };
  if (fields.contribution.trim() !== '') v.contribution = fields.contribution.trim();
  return ok(v);
}

export interface ConjectureFields {
  state: ConjectureState['state'];
  reason: string;
  outcome: SettlementOutcome;
  summary: string;
}

/** `PUT /conjectures/{claim}`. */
export function buildConjectureState(f: ConjectureFields): Parsed<ConjectureState> {
  switch (f.state) {
    case 'screening':
    case 'taken_up':
      return ok({ state: f.state });
    case 'not_pursued':
      return f.reason.trim() === ''
        ? fail('Give the reason it is not pursued.')
        : ok({ state: 'not_pursued', reason: f.reason.trim() });
    case 'settled':
      return f.summary.trim() === ''
        ? fail('Summarise how it was settled.')
        : ok({ state: 'settled', outcome: f.outcome, summary: f.summary.trim(), evidence: [] });
  }
}
