import { describe, expect, it } from 'vitest';
import type { StageReport } from '../api/types';
import { reports, submission } from '../test/fixtures';
import {
  currentReport,
  isMachineProposal,
  isValidForClaims,
  stageHistory,
  reportedStageState,
  stageViews,
} from './reports';
import { STAGES } from './stages';

describe('current vs superseded report selection', () => {
  it('takes the newest report for stages that do not depend on claims', () => {
    expect(currentReport(submission, 'hygiene')).toBe(reports[0]);
    // Claims reports are never voided by the claims revision they create.
    expect(currentReport(submission, 'claims')).toBe(reports[4]);
  });

  it('ignores claim-dependent reports filed against an older claims revision', () => {
    expect(currentReport(submission, 'literature')).toBe(reports[5]);
    expect(currentReport(submission, 'escape')).toBeUndefined();
    expect(isValidForClaims(reports[3] as StageReport, submission)).toBe(false);
    expect(isValidForClaims(reports[0] as StageReport, { claims_revision: 99 })).toBe(true);
  });

  it('labels history newest first as current, replaced or superseded', () => {
    expect(stageHistory(submission, 'claims').map((h) => [h.index, h.standing])).toEqual([
      [4, 'current'],
      [1, 'replaced'],
    ]);
    expect(stageHistory(submission, 'literature').map((h) => [h.index, h.standing])).toEqual([
      [5, 'current'],
      [2, 'superseded'],
    ]);
    expect(stageHistory(submission, 'escape').map((h) => h.standing)).toEqual(['superseded']);
  });

  it('prefers the newest valid report even when a later one is voided', () => {
    const base = reports[5] as StageReport;
    const s = {
      claims_revision: 2,
      reports: [
        { ...base, claims_revision: 2, summary: 'valid' },
        { ...base, claims_revision: 1, summary: 'stale but newer' },
      ],
    };
    expect(currentReport(s, 'literature')?.summary).toBe('valid');
    expect(stageHistory(s, 'literature').map((h) => h.standing)).toEqual(['superseded', 'current']);
  });

  it('marks a machine pass on a judgement stage as a proposal', () => {
    const views = stageViews(submission, STAGES);
    const states = Object.fromEntries(
      views.map((v) => [v.stage, reportedStageState(v, ['claims', 'literature', 'escape'])]),
    );
    expect(states).toEqual({
      hygiene: 'passed',
      claims: 'passed',
      literature: 'awaiting_human',
      escape: 'superseded',
    });
    expect(
      reportedStageState({ stage: 'escape', current: undefined, history: [] }, ['escape']),
    ).toBe('not_reported');
    expect(isMachineProposal(reports[5] as StageReport, ['literature'])).toBe(true);
    expect(isMachineProposal(reports[5] as StageReport, [])).toBe(false);
    expect(isMachineProposal(reports[2] as StageReport, ['literature'])).toBe(false);
  });
});
