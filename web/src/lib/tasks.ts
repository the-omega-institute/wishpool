import type {
  ContributionOutput,
  ContributionStatus,
  NewContribution,
  PriorWork,
  ProofShape,
  TaskKind,
  TaskState,
} from '../api/types';
import { safeHttpUrl } from './links';
import { fail, lines, ok, parseCount, type Parsed } from './parsed';

export const TASK_KINDS: readonly TaskKind[] = [
  'judge_escape',
  'literature_check',
  'formalize',
  'probe',
];

export const TASK_KIND_LABELS: { readonly [K in TaskKind]: string } = {
  judge_escape: 'Escape judgement',
  literature_check: 'Literature check',
  formalize: 'Formalization',
  probe: 'Conjecture probe',
};

/** What the contributor's agent does. */
export const TASK_KIND_DESCRIPTIONS: { readonly [K in TaskKind]: string } = {
  judge_escape:
    'Judge whether a proved statement is bind-only or carries content, naming its escape witnesses. Offered while the paper is in review.',
  literature_check:
    'Find prior work that states a main result, directly implies it, or is closely related, citing only works actually opened. Offered while the paper is in review.',
  formalize:
    'Prove a statement the author approved for formalization in Lean and open a pull request to the paper’s formalization repository. Offered after acceptance.',
  probe:
    'Work on a conjecture of the paper: restate it precisely, test small cases, find related results, and say what would settle it. Offered after acceptance.',
};

/** What makes a contribution of this kind count. */
export const TASK_KIND_VERIFICATION: { readonly [K in TaskKind]: string } = {
  judge_escape:
    'Counts when an editor confirms it, or when a judgement from a different model family and account agrees.',
  literature_check: 'Counts when an editor reviews and accepts it.',
  formalize:
    'Counts when an editor records the verified Lean proof (repository, commit, declarations, standard axioms only).',
  probe: 'Counts when an editor reviews and accepts it. Results go to the author first.',
};

/** Kinds an editor accepts or rejects by review (`POST /contributions/{id}/review`). */
export function isReviewedKind(kind: TaskKind): boolean {
  return kind === 'literature_check' || kind === 'probe';
}

export const TASK_STATES: readonly TaskState[] = ['open', 'leased', 'submitted', 'done', 'closed'];

export const TASK_STATE_LABELS: { readonly [K in TaskState]: string } = {
  open: 'Open',
  leased: 'Leased',
  submitted: 'Submitted',
  done: 'Done',
  closed: 'Closed',
};

export const CONTRIBUTION_STATE_LABELS: { readonly [K in ContributionStatus['state']]: string } = {
  submitted: 'Submitted',
  verified: 'Verified',
  rejected: 'Rejected',
};

export const MODE_LABELS = { own_agent: 'own agent', hosted: 'hosted (donated quota)' } as const;

/** The output kind each task kind submits. */
export const OUTPUT_FOR_KIND: { readonly [K in TaskKind]: ContributionOutput['output'] } = {
  judge_escape: 'judgement',
  literature_check: 'literature',
  formalize: 'pull_request',
  probe: 'probe_note',
};

/** Everything the submit form collects; each kind reads only its own fields. */
export interface ContributionFields {
  tool: string;
  model: string;
  inputTokens: string;
  outputTokens: string;
  // judge_escape
  shape: ProofShape;
  witnesses: string;
  rationale: string;
  // literature_check
  prior: PriorWork[];
  searched: string;
  summary: string;
  // probe
  note: string;
  // formalize
  url: string;
}

export function emptyContributionFields(): ContributionFields {
  return {
    tool: '',
    model: '',
    inputTokens: '',
    outputTokens: '',
    shape: 'content',
    witnesses: '',
    rationale: '',
    prior: [],
    searched: '',
    summary: '',
    note: '',
    url: '',
  };
}

function buildOutput(kind: TaskKind, f: ContributionFields): Parsed<ContributionOutput> {
  switch (kind) {
    case 'judge_escape': {
      const witnesses = lines(f.witnesses);
      if (f.rationale.trim() === '') return fail('Give the rationale for the judgement.');
      if (f.shape === 'content' && witnesses.length === 0) {
        return fail('A content judgement names at least one escape witness.');
      }
      return ok({
        output: 'judgement',
        shape: f.shape,
        witnesses: f.shape === 'content' ? witnesses : [],
        rationale: f.rationale.trim(),
      });
    }
    case 'literature_check': {
      const searched = lines(f.searched);
      if (searched.length === 0) return fail('List where you searched.');
      if (f.summary.trim() === '') return fail('Summarise what you found.');
      const bad = f.prior.findIndex((p) => p.source.locator.trim() === '');
      if (bad >= 0) return fail(`Prior work ${bad + 1} needs a locator.`);
      return ok({
        output: 'literature',
        prior: f.prior.map((p) => ({ ...p, note: p.note.trim() })),
        searched,
        summary: f.summary.trim(),
      });
    }
    case 'probe':
      return f.note.trim() === ''
        ? fail('Write the probe note.')
        : ok({ output: 'probe_note', note: f.note.trim() });
    case 'formalize': {
      const url = safeHttpUrl(f.url);
      return url === null
        ? fail('Give the pull request URL (http or https).')
        : ok({ output: 'pull_request', url: f.url.trim() });
    }
  }
}

/** The body of `POST /tasks/{id}/contributions` for a task of `kind`. */
export function buildContribution(kind: TaskKind, f: ContributionFields): Parsed<NewContribution> {
  if (f.tool.trim() === '' || f.model.trim() === '') {
    return fail('Name the agent tool and the model that did the work.');
  }
  const output = buildOutput(kind, f);
  if (!output.ok) return output;
  const input = parseCount(f.inputTokens);
  const outputTokens = parseCount(f.outputTokens);
  if (!input.ok) return input;
  if (!outputTokens.ok) return outputTokens;
  const body: NewContribution = {
    agent: { tool: f.tool.trim(), model: f.model.trim() },
    output: output.value,
  };
  if (input.value !== undefined || outputTokens.value !== undefined) {
    body.tokens = { input: input.value ?? 0, output: outputTokens.value ?? 0 };
  }
  return ok(body);
}
