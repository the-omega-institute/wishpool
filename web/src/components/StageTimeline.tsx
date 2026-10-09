import type { Decision, PolicyDocument, Stage, Submission } from '../api/types';
import { reportedStageState, stageViews } from '../lib/reports';
import { STAGES, STAGE_STATE_LABELS, stageInfo, type StageState } from '../lib/stages';
import { ReportView } from './payload';
import { Badge, type BadgeTone } from './ui';

const STATE_TONES: { [K in StageState]: BadgeTone } = {
  passed: 'good',
  failed: 'bad',
  needs_human: 'warn',
  awaiting_human: 'warn',
  not_reported: 'muted',
  superseded: 'muted',
};

const DEFAULT_HUMAN_JUDGEMENT: readonly Stage[] = ['claims', 'literature', 'escape'];

/** The stage a pending decision is waiting on, if any. */
export function awaitedStage(decision: Decision | null | undefined): Stage | null {
  return decision?.decision === 'pending' ? decision.awaiting : null;
}

/**
 * S0–S3 for one paper: the report each stage is currently judged by, and the
 * append-only history beneath it.
 */
export function StageTimeline({
  submission,
  policy,
  decision,
}: {
  submission: Submission;
  policy: PolicyDocument | null;
  decision: Decision | null;
}) {
  const humanJudgement = policy?.policy.human_judgement ?? [...DEFAULT_HUMAN_JUDGEMENT];
  const awaiting = awaitedStage(decision);
  const views = stageViews(submission, STAGES);

  return (
    <ol className="timeline" aria-label="Review stages">
      {views.map((view) => {
        const info = stageInfo(view.stage, policy);
        const isAwaited = awaiting === view.stage;
        const headingId = `stage-${view.stage}`;
        const state = reportedStageState(view, humanJudgement);
        const older = view.history.filter((h) => h.standing !== 'current');
        return (
          <li
            key={view.stage}
            className={`stage stage-${state} ${isAwaited ? 'stage-awaited' : ''}`}
            aria-labelledby={headingId}
          >
            <div className="stage-marker" aria-hidden="true">
              {info.code}
            </div>
            <div className="stage-body">
              <header className="stage-head">
                <h3 id={headingId}>
                  <span className="stage-code">{info.code}</span> {info.title}
                </h3>
                <Badge tone={STATE_TONES[state]}>{STAGE_STATE_LABELS[state]}</Badge>
                {isAwaited ? <Badge tone="accent">Awaiting</Badge> : null}
              </header>
              <p className="muted small">{info.description}</p>
              {isAwaited && decision?.decision === 'pending' ? (
                <p className="small">{decision.detail}</p>
              ) : null}
              {view.current ? (
                <ReportView
                  report={view.current}
                  claims={submission.claims}
                  humanJudgement={humanJudgement}
                  standing="current"
                  scope={`report-current-${view.stage}`}
                />
              ) : state === 'superseded' ? (
                <p className="muted">
                  Every report on this stage was filed against an earlier set of statements (current
                  revision {submission.claims_revision}); the stage is reviewed again.
                </p>
              ) : null}
              {older.length > 0 ? (
                <details className="history">
                  <summary>
                    {view.current ? 'Earlier reports' : 'Report history'} ({older.length})
                  </summary>
                  {older.map((h) => (
                    <ReportView
                      key={h.index}
                      report={h.report}
                      claims={submission.claims}
                      humanJudgement={humanJudgement}
                      standing={h.standing}
                      scope={`report-${h.index}`}
                    />
                  ))}
                </details>
              ) : null}
            </div>
          </li>
        );
      })}
    </ol>
  );
}
