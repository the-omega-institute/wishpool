import { describe, expect, it, vi } from 'vitest';
import {
  ApiError,
  buildQuery,
  createApiClient,
  parseErrorResponse,
  pdfHref,
  signInHref,
  sourceFileHref,
} from './client';

function problem(status: number, body: unknown, contentType = 'application/problem+json') {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': contentType } });
}

describe('parseErrorResponse', () => {
  it('reads code and detail from a problem+json body', async () => {
    const error = await parseErrorResponse(
      problem(409, {
        type: 'urn:wishpool:problem:conflict',
        title: 'conflict',
        status: 409,
        detail: 'awaiting S2: S2 needs a human report',
        code: 'conflict',
      }),
    );
    expect(error).toBeInstanceOf(ApiError);
    expect(error.status).toBe(409);
    expect(error.code).toBe('conflict');
    expect(error.detail).toBe('awaiting S2: S2 needs a human report');
    expect(error.title).toBe('conflict');
    expect(error.type).toBe('urn:wishpool:problem:conflict');
    expect(error.message).toBe(error.detail);
  });

  it('keeps stale_revision distinct from conflict on the same status', async () => {
    const error = await parseErrorResponse(
      problem(409, { code: 'stale_revision', detail: 'reload' }),
    );
    expect(error.code).toBe('stale_revision');
  });

  it('falls back to the status when the code is missing or unknown', async () => {
    expect((await parseErrorResponse(problem(422, { detail: 'title is empty' }))).code).toBe(
      'invalid',
    );
    expect((await parseErrorResponse(problem(403, { code: 'teapot', detail: 'x' }))).code).toBe(
      'forbidden',
    );
    expect((await parseErrorResponse(problem(401, {}))).code).toBe('not_authenticated');
    expect((await parseErrorResponse(problem(418, {}))).code).toBe('unknown');
  });

  it('uses the title when there is no detail', async () => {
    const error = await parseErrorResponse(problem(404, { title: 'not found', code: 'not_found' }));
    expect(error.detail).toBe('not found');
  });

  it('survives a non-JSON body from a proxy', async () => {
    const response = new Response('<html>Bad gateway</html>', {
      status: 502,
      statusText: 'Bad Gateway',
      headers: { 'content-type': 'text/html' },
    });
    const error = await parseErrorResponse(response);
    expect(error.status).toBe(502);
    expect(error.code).toBe('unavailable');
    expect(error.detail).toBe('Bad Gateway');
  });

  it('survives malformed JSON that claims to be a problem', async () => {
    const response = new Response('{not json', {
      status: 503,
      headers: { 'content-type': 'application/problem+json' },
    });
    const error = await parseErrorResponse(response);
    expect(error.code).toBe('unavailable');
    expect(error.detail).toMatch(/503/);
  });
});

describe('createApiClient', () => {
  it('loads referee rounds with an abort signal and sends feedback using the documented paths', async () => {
    const fetchImpl = vi.fn(async () => problem(200, {}, 'application/json'));
    const api = createApiClient(fetchImpl);
    const controller = new AbortController();
    await api.referee('s/1', { signal: controller.signal });
    await api.restartReferee('s/1');
    await api.sendFeedback('s/1', {
      subject: 'Edited subject',
      body: 'Edited body',
      note: 'Edited note',
    });
    const calls = fetchImpl.mock.calls as unknown as [string, RequestInit][];
    expect(calls.map(([url, init]) => `${init.method} ${url}`)).toEqual([
      'GET /api/v1/submissions/s%2F1/referee',
      'POST /api/v1/submissions/s%2F1/referee/restart',
      'POST /api/v1/submissions/s%2F1/referee/letters',
    ]);
    expect(calls[0]![1].signal).toBe(controller.signal);
    expect(JSON.parse(calls[2]![1].body as string)).toEqual({
      subject: 'Edited subject',
      body: 'Edited body',
      note: 'Edited note',
    });
    expect(calls.every(([, init]) => init.credentials === 'same-origin')).toBe(true);
    await api.sendFeedback('s/1', { subject: 'Subject', body: 'Body' });
    expect(
      JSON.parse(
        (fetchImpl.mock.calls as unknown as [string, RequestInit][])[3]![1].body as string,
      ),
    ).toEqual({ subject: 'Subject', body: 'Body' });
  });

  it('sends same-origin credentials and a JSON body', async () => {
    const fetchImpl = vi.fn(async () => problem(200, { id: 's1' }, 'application/json'));
    const api = createApiClient(fetchImpl);
    await api.confirmClaims('s1', [{ id: 'C1', kind: 'theorem', role: 'main', depends_on: [] }]);
    const [url, init] = fetchImpl.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/v1/submissions/s1/claims');
    expect(init.method).toBe('POST');
    expect(init.credentials).toBe('same-origin');
    expect((init.headers as Record<string, string>)['Content-Type']).toBe('application/json');
    expect(JSON.parse(init.body as string)).toEqual({
      claims: [{ id: 'C1', kind: 'theorem', role: 'main', depends_on: [] }],
    });
  });

  it('uploads a paper as multipart with metadata and source, without setting Content-Type', async () => {
    const fetchImpl = vi.fn(async () => problem(201, { id: 's1' }, 'application/json'));
    const api = createApiClient(fetchImpl);
    const file = new File(['\\documentclass{article}'], 'paper.tex', { type: 'application/x-tex' });
    const metadata = {
      ai_disclosure: { level: 'none' as const, statement: 'No AI was used.' },
      msc: ['11B83'],
      doi: null,
      open_to_contributors: true,
    };
    await api.createSubmission(metadata, file);
    const [url, init] = fetchImpl.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/v1/submissions');
    expect(init.method).toBe('POST');
    expect((init.headers as Record<string, string>)['Content-Type']).toBeUndefined();
    const body = init.body as FormData;
    expect(body).toBeInstanceOf(FormData);
    expect([...body.keys()]).toEqual(['metadata', 'source']);
    expect(JSON.parse(body.get('metadata') as string)).toEqual(metadata);
    const source = body.get('source') as File;
    expect(source.name).toBe('paper.tex');
    expect(await source.text()).toBe('\\documentclass{article}');
  });

  it('uploads a new version with source and note', async () => {
    const fetchImpl = vi.fn(async () => problem(200, { id: 's1' }, 'application/json'));
    const api = createApiClient(fetchImpl);
    await api.uploadVersion('s 1', new File(['x'], 'v2.zip'), 'Fixed Lemma 2');
    const [url, init] = fetchImpl.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/v1/submissions/s%201/versions');
    const body = init.body as FormData;
    expect((body.get('source') as File).name).toBe('v2.zip');
    expect(body.get('note')).toBe('Fixed Lemma 2');
    expect(body.get('metadata')).toBeNull();
  });

  it('builds the v3 paths', async () => {
    const fetchImpl = vi.fn(async () => problem(200, {}, 'application/json'));
    const api = createApiClient(fetchImpl);
    await api.listSubmissions('queue', { before: null });
    await api.fileReport('a b', 'literature', {
      report: {
        outcome: { outcome: 'pass' },
        summary: '',
        payload: { stage: 'literature', prior: [], searched: [] },
      },
    });
    await api.judgeClaim('s1', 'C1', { shape: 'content', witnesses: ['w'], rationale: 'r' });
    await api.adoptJudgements('s1');
    await api.setVisibility('s1', 'public');
    await api.setContributors('s1', false);
    await api.respondFormalization('s1', 'C1', { approve: false, reason: 'not yet' });
    await api.verifyFormalization('s1', 'C1', {
      artifact: {
        repository: 'https://github.com/a/b',
        commit: 'f'.repeat(40),
        declarations: ['x'],
      },
      axioms: [],
    });
    await api.updateConjecture('s1', 'C3', { state: 'taken_up' });
    await api.listPapers({ limit: 5 });
    await api.getPaper('WP-2026-0001');
    await api.listTasks({ kind: 'probe', status: 'open', submission: 's1' });
    const calls = fetchImpl.mock.calls.map((c) => c as unknown as [string, RequestInit]);
    expect(calls.map(([url, init]) => `${init.method} ${url}`)).toEqual([
      'GET /api/v1/submissions?scope=queue',
      'POST /api/v1/submissions/a%20b/stages/literature/reports',
      'POST /api/v1/submissions/s1/claims/C1/judgements',
      'POST /api/v1/submissions/s1/escape/adopt',
      'PUT /api/v1/submissions/s1/visibility',
      'PUT /api/v1/submissions/s1/contributors',
      'POST /api/v1/submissions/s1/formalization/items/C1/response',
      'POST /api/v1/submissions/s1/formalization/items/C1/verification',
      'PUT /api/v1/submissions/s1/conjectures/C3',
      'GET /api/v1/papers?limit=5',
      'GET /api/v1/papers/WP-2026-0001',
      'GET /api/v1/tasks?kind=probe&status=open&submission=s1',
    ]);
    expect(JSON.parse(calls[4]?.[1].body as string)).toEqual({ visibility: 'public' });
    expect(JSON.parse(calls[5]?.[1].body as string)).toEqual({ open: false });
    expect(JSON.parse(calls[8]?.[1].body as string)).toEqual({ state: { state: 'taken_up' } });
  });

  it('links the PDF and the source of a version', () => {
    expect(pdfHref('s1', 2)).toBe('/api/v1/submissions/s1/files/pdf?version=2');
    expect(pdfHref('s1')).toBe('/api/v1/submissions/s1/files/pdf');
    expect(sourceFileHref('s/1', 1)).toBe('/api/v1/submissions/s%2F1/files/source?version=1');
  });

  it('throws a typed ApiError for problem responses', async () => {
    const api = createApiClient(async () =>
      problem(409, { code: 'conflict', detail: 'awaiting S2: S2 needs a human report' }),
    );
    await expect(api.applyDecision('s1')).rejects.toMatchObject({
      name: 'ApiError',
      status: 409,
      code: 'conflict',
      detail: 'awaiting S2: S2 needs a human report',
    });
  });

  it('maps a network failure to code "network"', async () => {
    const api = createApiClient(async () => {
      throw new TypeError('Failed to fetch');
    });
    await expect(api.policy()).rejects.toMatchObject({ status: 0, code: 'network' });
  });

  it('returns undefined for 204 (logout)', async () => {
    const api = createApiClient(async () => new Response(null, { status: 204 }));
    await expect(api.logout()).resolves.toBeUndefined();
  });

  it('reads the session from /auth/session', async () => {
    const fetchImpl = vi.fn(async () => problem(200, { authenticated: false }, 'application/json'));
    const api = createApiClient(fetchImpl);
    await expect(api.session()).resolves.toEqual({ authenticated: false });
    expect((fetchImpl.mock.calls[0] as unknown as [string])[0]).toBe('/auth/session');
  });
});

describe('contributor routes', () => {
  function recording(body: unknown = {}) {
    const fetchImpl = vi.fn(async () => problem(200, body, 'application/json'));
    return { fetchImpl, api: createApiClient(fetchImpl) };
  }
  const call = (fetchImpl: ReturnType<typeof vi.fn>, i: number) =>
    fetchImpl.mock.calls[i] as unknown as [string, RequestInit];

  it('leases with an empty body or a mode, releases with DELETE and patches donations', async () => {
    const { fetchImpl, api } = recording();
    await api.leaseTask('t1');
    await api.leaseTask('t1', 'hosted');
    await api.releaseTask('t1');
    await api.updateDonation({ status: 'paused' });
    await api.listContributions({ task: 't 1' });
    expect(call(fetchImpl, 0)[1].body).toBe('{}');
    expect(JSON.parse(call(fetchImpl, 1)[1].body as string)).toEqual({ mode: 'hosted' });
    expect(call(fetchImpl, 2)[1].method).toBe('DELETE');
    expect(call(fetchImpl, 2)[1].body).toBeUndefined();
    expect(call(fetchImpl, 3)[1].method).toBe('PATCH');
    expect(JSON.parse(call(fetchImpl, 3)[1].body as string)).toEqual({ status: 'paused' });
    expect(call(fetchImpl, 4)[0]).toBe('/api/v1/contributions?task=t+1');
  });

  it('returns null when there is no donation grant', async () => {
    const api = createApiClient(async () => problem(200, null, 'application/json'));
    await expect(api.getDonation()).resolves.toBeNull();
  });
});

describe('helpers', () => {
  it('omits empty query values', () => {
    expect(buildQuery({ a: '', b: undefined, c: null, d: 0, e: 'x y' })).toBe('?d=0&e=x+y');
    expect(buildQuery({})).toBe('');
  });

  it('builds the sign-in URL with an encoded return path', () => {
    expect(signInHref('/submissions/s1?x=1')).toBe(
      '/auth/login?return_to=%2Fsubmissions%2Fs1%3Fx%3D1',
    );
  });
});
