import type { Stage, StageReport, Submission } from '../api/types';
import { dependsOnClaims, type StageState } from './stages';

export type ReportStanding =
  /** The report the admission policy reads for its stage. */
  | 'current'
  /** A later report on the same stage replaced it. */
  | 'replaced'
  /** Filed against an earlier claim set; voided when the claims changed. */
  | 'superseded';

export interface AnnotatedReport {
  report: StageReport;
  /** Position in the submission's append-only history. */
  index: number;
  standing: ReportStanding;
}

type ClaimsRevision = Pick<Submission, 'claims_revision'>;

/**
 * Whether a report still counts. Claim-dependent stages (literature, escape)
 * are judged against a particular claim set, so their reports
 * lapse when a new claims report changes the claims. This mirrors the
 * server's `Submission::latest_report`.
 */
export function isValidForClaims(report: StageReport, submission: ClaimsRevision): boolean {
  return !dependsOnClaims(report.stage) || report.claims_revision === submission.claims_revision;
}

/** The report the policy reads for `stage`: the newest one still valid for the claims. */
export function currentReport(
  submission: Pick<Submission, 'claims_revision' | 'reports'>,
  stage: Stage,
): StageReport | undefined {
  for (let i = submission.reports.length - 1; i >= 0; i -= 1) {
    const report = submission.reports[i];
    if (report && report.stage === stage && isValidForClaims(report, submission)) return report;
  }
  return undefined;
}

/** Every report on `stage`, newest first, with its standing. */
export function stageHistory(
  submission: Pick<Submission, 'claims_revision' | 'reports'>,
  stage: Stage,
): AnnotatedReport[] {
  const current = currentReport(submission, stage);
  const history: AnnotatedReport[] = [];
  submission.reports.forEach((report, index) => {
    if (report.stage !== stage) return;
    const standing: ReportStanding =
      report === current
        ? 'current'
        : isValidForClaims(report, submission)
          ? 'replaced'
          : 'superseded';
    history.push({ report, index, standing });
  });
  return history.reverse();
}

export interface StageView {
  stage: Stage;
  current: StageReport | undefined;
  history: AnnotatedReport[];
}

export function stageViews(
  submission: Pick<Submission, 'claims_revision' | 'reports'>,
  stages: readonly Stage[],
): StageView[] {
  return stages.map((stage) => ({
    stage,
    current: currentReport(submission, stage),
    history: stageHistory(submission, stage),
  }));
}

/**
 * On judgement stages a machine `pass` only proposes; the stage still waits
 * for a human report.
 */
export function isMachineProposal(report: StageReport, humanJudgement: readonly Stage[]): boolean {
  return (
    report.reviewer.kind === 'machine' &&
    report.outcome.outcome === 'pass' &&
    humanJudgement.includes(report.stage)
  );
}

/** The state of a stage, read from its current report. */
export function reportedStageState(view: StageView, humanJudgement: readonly Stage[]): StageState {
  const report = view.current;
  if (!report) return view.history.length > 0 ? 'superseded' : 'not_reported';
  switch (report.outcome.outcome) {
    case 'fail':
      return 'failed';
    case 'needs_human':
      return 'needs_human';
    case 'pass':
      return isMachineProposal(report, humanJudgement) ? 'awaiting_human' : 'passed';
  }
}
