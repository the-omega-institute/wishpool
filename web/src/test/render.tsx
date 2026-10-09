import { render } from '@testing-library/react';
import type { ReactElement } from 'react';
import { vi } from 'vitest';
import { ApiError, type ApiClient } from '../api/client';
import { ApiProvider } from '../api/context';
import { PolicyProvider } from '../api/policy';
import type { Person } from '../api/types';
import { SessionProvider } from '../auth/session';
import { RouterProvider } from '../routing/router';
import { policy } from './fixtures';

const notFound = () =>
  Promise.reject(new ApiError({ status: 404, code: 'not_found', detail: 'not stubbed' }));
const empty = async () => ({ items: [], next_before: null });

/** A client whose every call fails unless overridden. */
export function fakeApi(overrides: Partial<ApiClient> = {}, person?: Person): ApiClient {
  const base: ApiClient = {
    session: vi.fn(async () =>
      person ? { authenticated: true as const, person } : { authenticated: false as const },
    ),
    logout: vi.fn(async () => undefined),
    policy: vi.fn(async () => policy),
    me: vi.fn(notFound),
    listPeople: vi.fn(empty),
    setRoles: vi.fn(notFound),
    createSubmission: vi.fn(notFound),
    listSubmissions: vi.fn(empty),
    getSubmission: vi.fn(notFound),
    confirmClaims: vi.fn(notFound),
    uploadVersion: vi.fn(notFound),
    withdrawSubmission: vi.fn(notFound),
    setContributors: vi.fn(notFound),
    setVisibility: vi.fn(notFound),
    getAnalysis: vi.fn(notFound),
    previewDecision: vi.fn(notFound),
    referee: vi.fn(async (id: string) => ({ id, rounds: [], letters: [], revision: 0 })),
    respondFormalization: vi.fn(notFound),
    fileReport: vi.fn(notFound),
    judgeClaim: vi.fn(notFound),
    adoptJudgements: vi.fn(notFound),
    applyDecision: vi.fn(notFound),
    setFormalRepository: vi.fn(notFound),
    proposeFormalization: vi.fn(notFound),
    startFormalization: vi.fn(notFound),
    verifyFormalization: vi.fn(notFound),
    updateConjecture: vi.fn(notFound),
    generateTasks: vi.fn(notFound),
    restartReferee: vi.fn(notFound),
    sendFeedback: vi.fn(notFound),
    listPapers: vi.fn(empty),
    getPaper: vi.fn(notFound),
    listTasks: vi.fn(empty),
    getTask: vi.fn(notFound),
    leaseTask: vi.fn(notFound),
    releaseTask: vi.fn(notFound),
    submitContribution: vi.fn(notFound),
    listContributions: vi.fn(empty),
    reviewContribution: vi.fn(notFound),
    listContributors: vi.fn(async () => []),
    getDonation: vi.fn(async () => null),
    updateDonation: vi.fn(notFound),
  };
  return { ...base, ...overrides };
}

export function renderWithApp(ui: ReactElement, api: ApiClient) {
  return render(
    <ApiProvider client={api}>
      <SessionProvider>
        <PolicyProvider>
          <RouterProvider>{ui}</RouterProvider>
        </PolicyProvider>
      </SessionProvider>
    </ApiProvider>,
  );
}
