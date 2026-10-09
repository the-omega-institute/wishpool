import type {
  Advice,
  FeedbackLetter,
  FormalProbe,
  LetterDraft,
  RefereeFile,
  RefereeReport,
  RefereeRound,
} from '../api/types';
import { editor, submission } from './fixtures';

export const refereeReport: RefereeReport = {
  recommendation: 'minor_revision',
  summary: 'The argument is promising; the boundary case needs explanation.',
  strengths: ['A clear induction argument.'],
  concerns: [
    { claim: 'C1', severity: 'major', issue: 'Explain the induction base.' },
    { claim: 'C2', severity: 'minor', issue: 'Clarify the domain.' },
  ],
  claims: [
    {
      claim: 'C1',
      shape: 'content',
      witnesses: ['A proposed intermediate identity.'],
      note: 'Check the boundary case.',
    },
    {
      claim: 'C2',
      shape: 'bind_only',
      witnesses: [],
      known: 'A standard order argument.',
      note: 'Instantiates monotonicity.',
    },
  ],
  limits: ['The referee did not check every literature source.'],
  text: 'Full answer: the map $\\rep$ acts on $\\code{ab}$.',
};

export const refereeAdvice: Advice = {
  summary: 'Contributors can check the base case and clarify the exposition.',
  improvements: [
    {
      claim: 'C1',
      kind: 'gap',
      suggestion: 'Add the n = 1 calculation.',
      how_we_help: 'Supply a short checked calculation.',
      effort: 'small',
      status: 'checked',
      evidence: 'For n = 1 both sides equal 1.',
    },
    {
      kind: 'exposition',
      suggestion: 'Add a diagram.',
      how_we_help: 'Prepare an explanatory figure.',
      effort: 'medium',
      status: 'proposed',
      evidence: '',
    },
  ],
  formalization: [
    {
      claim: 'C2',
      feasibility: 'ready',
      mathlib: ['Nat', 'StrictMono'],
      missing: ['A wrapper lemma.'],
      lean_sketch:
        'theorem square_injective : Function.Injective (fun n : ℕ => n * n) := by\n  intro a b h',
      plan: 'Prove strict monotonicity on natural numbers, then obtain injectivity.',
      effort: 'small',
    },
  ],
};

export const letterDraft: LetterDraft = {
  subject: 'Feedback on your paper',
  body: 'Please explain the **base case** for $\\rep$.',
  note: 'The suggested formalization uses $\\code{Nat}$.',
};

export const feedbackLetter: FeedbackLetter = {
  round: 1,
  subject: 'Sent editorial feedback',
  body: 'Please clarify the base case for $\\rep$.',
  note: 'We can help with $\\code{Nat}$.',
  edited: true,
  sent_by: editor.id,
  sent_at: '2026-10-08T12:00:00Z',
};

export const formalProbe: FormalProbe = {
  toolchain: 'leanprover/lean4:v4.33.0, Mathlib v4.33.0',
  summary: 'Squares are injective on the naturals.',
  attempts: [
    {
      claim: 'C2',
      outcome: 'compiled',
      theorem: 'Wishpool.C2.main',
      lean: 'import Mathlib\n\ntheorem Wishpool.C2.main : Function.Injective (fun n : ℕ => n * n) := by\n  exact?',
      axioms: ['propext', 'Classical.choice', 'Quot.sound'],
      note: 'Matches the paper’s statement for natural numbers.',
      log: '',
    },
  ],
};

export const refereeRound: RefereeRound = {
  number: 1,
  version: 1,
  claims_revision: submission.claims_revision,
  started_at: '2026-10-08T08:00:00Z',
  referee: {
    engine: 'NyxID Oracle',
    model: 'ChatGPT Pro',
    attempts: 1,
    state: { state: 'done', result: refereeReport, at: '2026-10-08T10:00:00Z' },
  },
  advice: {
    engine: 'Codex',
    model: 'gpt-6',
    attempts: 1,
    state: { state: 'done', result: refereeAdvice, at: '2026-10-08T11:00:00Z' },
  },
  formal: {
    engine: 'codex-cli+lean',
    model: 'gpt-6',
    attempts: 1,
    state: { state: 'done', result: formalProbe, at: '2026-10-08T11:20:00Z' },
  },
  letter: {
    engine: 'Codex',
    model: 'gpt-6',
    attempts: 1,
    state: { state: 'done', result: letterDraft, at: '2026-10-08T11:30:00Z' },
  },
};

export function refereeFile(extra: Partial<RefereeFile> = {}): RefereeFile {
  return { id: submission.id, rounds: [refereeRound], letters: [], revision: 1, ...extra };
}

export const queuedRound: RefereeRound = {
  ...refereeRound,
  referee: {
    ...refereeRound.referee,
    state: { state: 'running', queue_position: 7, since: refereeRound.started_at },
  },
  advice: { attempts: 0, state: { state: 'pending' } },
  formal: { attempts: 0, state: { state: 'pending' } },
  letter: { attempts: 0, state: { state: 'pending' } },
};
