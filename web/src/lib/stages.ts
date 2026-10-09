import type { PolicyDocument, PolicyStage, ReportedStage, Stage } from '../api/types';

export const STAGES: readonly Stage[] = ['hygiene', 'claims', 'literature', 'escape'];

/** Stages editors and reviewer accounts file reports on. */
export const REPORTED_STAGES: readonly ReportedStage[] = ['hygiene', 'literature', 'escape'];

/** Stages whose reports are judged against a particular claim set. */
export const CLAIM_DEPENDENT_STAGES: readonly Stage[] = ['literature', 'escape'];

export function dependsOnClaims(stage: Stage): boolean {
  return CLAIM_DEPENDENT_STAGES.includes(stage);
}

/** Local descriptions (the server's text), used until or unless `GET /policy` answers. */
export const DEFAULT_STAGE_INFO: readonly PolicyStage[] = [
  {
    stage: 'hygiene',
    code: 'S0',
    title: 'Source',
    description: 'The LaTeX source is read, the PDF compiles, and AI use is disclosed.',
  },
  {
    stage: 'claims',
    code: 'S1',
    title: 'Statements',
    description:
      'The author confirms the theorems, lemmas and conjectures read from the source and marks the main results.',
  },
  {
    stage: 'literature',
    code: 'S2',
    title: 'Literature',
    description: 'No main result is already stated in, or directly implied by, prior work.',
  },
  {
    stage: 'escape',
    code: 'S3',
    title: 'Escape analysis',
    description:
      'Each proved statement is judged bind-only or content. A content statement names escape witnesses: new propositions on its proof path that prior results do not give by instantiation, projection or normalisation.',
  },
];

/** The publication threshold, as stated in `docs/API.md`. */
export const THRESHOLD_TEXT =
  'A paper is accepted when some main result carries an escape witness (judged content) and the literature check found no work that states it or directly implies it; or when a main result settles a named, sourced open problem that the literature had not settled.';

export function stageInfo(stage: Stage, policy?: PolicyDocument | null): PolicyStage {
  const published = policy?.stages.find((s) => s.stage === stage);
  if (published) return published;
  const local = DEFAULT_STAGE_INFO.find((s) => s.stage === stage);
  return local ?? { stage, code: '', title: stage, description: '' };
}

/** `S3 Escape analysis`. */
export function stageName(stage: Stage, policy?: PolicyDocument | null): string {
  const info = stageInfo(stage, policy);
  return info.code ? `${info.code} ${info.title}` : info.title;
}

export type StageState =
  'passed' | 'failed' | 'needs_human' | 'awaiting_human' | 'not_reported' | 'superseded';

export const STAGE_STATE_LABELS: { readonly [K in StageState]: string } = {
  passed: 'Passed',
  failed: 'Failed',
  needs_human: 'Needs an editor',
  awaiting_human: 'Machine pass — awaiting an editor',
  not_reported: 'No report yet',
  superseded: 'Reports superseded by new statements',
};
