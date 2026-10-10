import type {
  Claim,
  DonationGrant,
  PaperAnalysis,
  PaperSummary,
  PaperVersion,
  Person,
  PolicyDocument,
  PublicPaper,
  StageReport,
  Submission,
  Task,
  TaskContext,
} from '../api/types';
import { DEFAULT_STAGE_INFO } from '../lib/stages';

export const SHA = '0123456789abcdef0123456789abcdef01234567';

function person(id: string, name: string, roles: Person['roles'] = []): Person {
  return {
    id,
    display_name: name,
    roles,
    created_at: '2026-01-01T00:00:00Z',
    last_seen_at: '2026-10-01T00:00:00Z',
  };
}

export const editor = person('nyx|editor-1', 'Emmy Editor', ['editor']);
export const author = person('nyx|author-1', 'Ada Author');
export const contributor = person('nyx|contrib-1', 'Carl Contributor');
export const donor = person('nyx|donor-1', 'Dora Donor');

export const policy: PolicyDocument = {
  policy: {
    human_judgement: ['claims', 'literature', 'escape'],
    required_endorsements: 0,
    max_active_per_author: 3,
  },
  stages: [...DEFAULT_STAGE_INFO],
};

/** Read from the source: a theorem using a lemma, a conjecture, and a misparsed fragment. */
export const extracted: Claim[] = [
  {
    id: 'C1',
    kind: 'theorem',
    label: 'Theorem 1.1',
    latex_label: 'thm:main',
    statement: 'For every $n \\ge 1$ we have \\[ \\sum_{k=1}^n k = \\frac{n(n+1)}{2}. \\]',
    role: 'main',
    has_proof: true,
    section: 'Introduction',
    depends_on: [],
  },
  {
    id: 'C2',
    kind: 'lemma',
    label: 'Lemma 2.1',
    statement: 'The map $x \\mapsto x^2$ is \\emph{injective} on $\\mathbb{N}$.',
    role: 'supporting',
    has_proof: true,
    depends_on: [],
  },
  {
    id: 'C3',
    kind: 'conjecture',
    label: 'Conjecture 4.1',
    statement: 'There are infinitely many primes $p$ with $p + 2$ prime.',
    role: 'supporting',
    has_proof: false,
    depends_on: [],
  },
  {
    id: 'C4',
    kind: 'claim',
    label: '',
    statement: '\\label{junk} see above',
    role: 'supporting',
    has_proof: false,
    depends_on: [],
  },
];

/** The author's confirmation of `extracted` (C4 excluded, C1 uses C2). */
export const claims: Claim[] = [
  { ...(extracted[0] as Claim), depends_on: ['C2'] },
  extracted[1] as Claim,
  extracted[2] as Claim,
];

export function version(number: number, extra: Partial<PaperVersion> = {}): PaperVersion {
  return {
    number,
    archive: { id: `blob-${number}`, bytes: 52_000, sha256: SHA },
    filename: 'paper.tar.gz',
    main_file: 'main.tex',
    pdf: { id: `pdf-${number}`, bytes: 310_000, sha256: SHA },
    parse_warnings: [],
    macros: {},
    note: number === 1 ? 'Initial upload' : 'Revised',
    uploaded_at: '2026-10-01T12:00:00Z',
    ...extra,
  };
}

const machine = {
  kind: 'machine' as const,
  account: 'nyx|robot',
  engine: 'automath',
  model: 'm-7',
};
const human = { kind: 'human' as const, person: editor.id };

export const reports: StageReport[] = [
  {
    stage: 'hygiene',
    outcome: { outcome: 'pass' },
    summary: 'The source compiles.',
    payload: {
      stage: 'hygiene',
      checks: [
        { name: 'pdf_compiles', passed: true, detail: '' },
        { name: 'ai_disclosure_present', passed: true, detail: '' },
      ],
    },
    evidence: [],
    reviewer: machine,
    claims_revision: 0,
    filed_at: '2026-10-01T12:05:00Z',
  },
  {
    stage: 'claims',
    outcome: { outcome: 'pass' },
    summary: '3 statements confirmed by the author.',
    payload: { stage: 'claims', claims: [extracted[0] as Claim] },
    evidence: [],
    reviewer: { kind: 'human', person: author.id },
    claims_revision: 0,
    filed_at: '2026-10-01T12:10:00Z',
  },
  {
    stage: 'literature',
    outcome: { outcome: 'pass' },
    summary: 'Old leads.',
    payload: { stage: 'literature', prior: [], searched: ['OpenAlex'] },
    evidence: [],
    reviewer: human,
    claims_revision: 0,
    filed_at: '2026-10-01T13:00:00Z',
  },
  {
    stage: 'escape',
    outcome: { outcome: 'pass' },
    summary: 'Old escape analysis.',
    payload: { stage: 'escape', assessments: [] },
    evidence: [],
    reviewer: human,
    claims_revision: 0,
    filed_at: '2026-10-01T13:30:00Z',
  },
  {
    stage: 'claims',
    outcome: { outcome: 'pass' },
    summary: '3 statements confirmed by the author.',
    payload: { stage: 'claims', claims },
    evidence: [],
    reviewer: { kind: 'human', person: author.id },
    claims_revision: 1,
    filed_at: '2026-10-02T09:00:00Z',
  },
  {
    stage: 'literature',
    outcome: { outcome: 'pass' },
    summary: 'Machine leads from OpenAlex.',
    payload: {
      stage: 'literature',
      prior: [
        {
          claim: 'C1',
          source: { kind: 'doi', locator: '10.1000/xyz', year: 1999 },
          relation: 'related',
          note: 'A weaker bound.',
        },
      ],
      searched: ['OpenAlex'],
    },
    evidence: [],
    reviewer: machine,
    claims_revision: 1,
    filed_at: '2026-10-02T10:00:00Z',
  },
];

export const submission: Submission = {
  kind: 'paper',
  lean_statements: [],
  id: 'sub-1',
  submitter: author.id,
  title: 'On sums of integers',
  abstract_text: 'We prove $\\sum k = n(n+1)/2$.',
  authors: [{ name: 'Ada Author', person: author.id }, { name: 'Ben Second' }],
  ai_disclosure: { level: 'assisted', statement: 'A model checked the algebra.' },
  msc: ['11B83'],
  versions: [version(1)],
  extracted,
  claims,
  claims_revision: 1,
  reports,
  status: { state: 'in_review' },
  decision: { decision: 'pending', awaiting: 'literature', detail: 'S2 needs a human report' },
  open_to_contributors: true,
  analysis_visibility: 'undecided',
  formalization: { items: [] },
  conjectures: [],
  created_at: '2026-10-01T12:00:00Z',
  updated_at: '2026-10-02T10:00:00Z',
  revision: 7,
};

export const draft: Submission = {
  ...submission,
  id: 'sub-draft',
  versions: [
    version(1, { parse_warnings: ['\\newtheorem{conj} has no number; using "Conjecture"'] }),
  ],
  claims: [],
  claims_revision: 0,
  reports: [reports[0] as StageReport],
  status: { state: 'draft' },
  decision: {
    decision: 'pending',
    awaiting: 'claims',
    detail: 'the author has not confirmed the statements',
  },
  revision: 2,
};

export const notAccepted: Submission = {
  ...submission,
  id: 'sub-na',
  status: { state: 'not_accepted' },
  decision: {
    decision: 'not_accepted',
    reasons: [
      { reason: 'known_result', claim: 'C1', prior: { kind: 'arxiv', locator: '1901.00001' } },
      { reason: 'bind_only' },
    ],
  },
  revision: 12,
};

export const accepted: Submission = {
  ...submission,
  id: 'sub-acc',
  status: { state: 'accepted', record: 'WP-2026-0001' },
  decision: { decision: 'accept', basis: 'escape_witness' },
  analysis_visibility: 'undecided',
  formalization: {
    repository: 'https://github.com/ada/sums-lean',
    items: [
      {
        claim: 'C1',
        reason: 'The main identity is short to state in Mathlib.',
        state: { state: 'proposed' },
        updated_at: '2026-10-05T00:00:00Z',
      },
    ],
  },
  conjectures: [{ claim: 'C3', state: { state: 'screening' }, updated_at: '2026-10-05T00:00:00Z' }],
  revision: 20,
};

export const analysis: PaperAnalysis = {
  submission: 'sub-acc',
  main_results: 1,
  main_with_content: 1,
  main_known: 0,
  open_statements: 1,
  formalized: 1,
  claims: [
    {
      claim: 'C1',
      kind: 'theorem',
      role: 'main',
      label: 'Theorem 1.1',
      prior: [
        {
          claim: 'C1',
          source: { kind: 'doi', locator: '10.1000/xyz', year: 1999 },
          relation: 'related',
          note: 'A weaker bound.',
        },
      ],
      known: false,
      assessment: {
        claim: 'C1',
        shape: 'content',
        witnesses: ['The pairing $k \\leftrightarrow n+1-k$.'],
        rationale: 'The pairing is new on the proof path.',
      },
      judgement: {
        shape: 'content',
        standing: 'confirmed',
        witnesses: ['The pairing $k \\leftrightarrow n+1-k$.'],
        judgements: 2,
      },
      formalization: {
        state: 'verified',
        artifact: {
          repository: 'https://github.com/ada/sums-lean',
          commit: SHA,
          declarations: ['Sums.main'],
        },
        axioms: ['propext', 'Classical.choice', 'Quot.sound'],
      },
    },
  ],
};

export const paperSummary: PaperSummary = {
  kind: 'paper',
  record: 'WP-2026-0001',
  submission: 'sub-acc',
  title: 'On sums of integers',
  authors: [{ name: 'Ada Author' }, { name: 'Ben Second' }],
  abstract_text: 'We prove $\\sum k = n(n+1)/2$.',
  msc: ['11B83'],
  doi: '10.48550/arXiv.2609.33421',
  basis: 'escape_witness',
  accepted_at: '2026-10-05T00:00:00Z',
  main_results: 1,
  new_results: 1,
  lean_verified: 1,
};

export const publicPaper: PublicPaper = {
  summary: paperSummary,
  versions: [{ number: 1, uploaded_at: '2026-10-01T12:00:00Z', has_pdf: true }],
  claims: [
    {
      id: 'C1',
      kind: 'theorem',
      role: 'main',
      label: 'Theorem 1.1',
      depends_on: [],
      statement: 'For every $n \\ge 1$ we have $\\sum_{k=1}^n k = n(n+1)/2$.',
      lean: {
        repository: 'https://github.com/ada/sums-lean',
        commit: SHA,
        declarations: ['Sums.main'],
      },
    },
    {
      id: 'C3',
      kind: 'conjecture',
      role: 'supporting',
      label: 'Conjecture 4.1',
      depends_on: [],
      statement: 'There are infinitely many twin primes.',
    },
  ],
  formalization_repository: 'https://github.com/ada/sums-lean',
  macros: {},
  new_content: [],
};

export const task: Task = {
  id: 'task-1',
  kind: 'judge_escape',
  target: { submission: 'sub-1', claim: 'C1' },
  dedupe_key: 'judge_escape:sub-1:C1:1',
  title: 'Judge the escape of Theorem 1.1',
  contributors: [],
  status: {
    state: 'leased',
    lease: { holder: contributor.id, mode: 'own_agent', until: '2026-10-08T12:00:00Z' },
  },
  created_by: editor.id,
  created_at: '2026-10-02T11:00:00Z',
  updated_at: '2026-10-08T10:00:00Z',
  revision: 3,
};

export const taskContext: TaskContext = {
  task,
  paper_title: 'On sums of integers',
  abstract_text: 'We prove $\\sum k = n(n+1)/2$.',
  claim: claims[0] as Claim,
  dependencies: [claims[1] as Claim],
  pdf_public: false,
  nature: 'A judgement with provenance; it counts when corroborated or confirmed.',
  macros: {},
};

export const grant: DonationGrant = {
  donor: donor.id,
  monthly_cap: 200_000,
  model: 'claude-opus-5-5',
  period: '2026-10',
  used: 50_000,
  status: 'active',
  created_at: '2026-10-01T00:00:00Z',
  updated_at: '2026-10-05T00:00:00Z',
  revision: 2,
};
