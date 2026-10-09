import { describe, expect, it } from 'vitest';
import { MAX_SOURCE_BYTES, buildNewPaper, isDoi, sourceFileProblem } from './upload';

const base = {
  aiLevel: 'assisted' as const,
  aiStatement: ' A model checked the algebra. ',
  msc: '',
  doi: '',
  openToContributors: false,
};

describe('upload checks', () => {
  it('accepts .tex, .zip and .tar.gz up to 30 MB', () => {
    expect(sourceFileProblem({ name: 'paper.tex', size: 10 })).toBeNull();
    expect(sourceFileProblem({ name: 'Paper.ZIP', size: 10 })).toBeNull();
    expect(sourceFileProblem({ name: 'paper.tar.gz', size: MAX_SOURCE_BYTES })).toBeNull();
    expect(sourceFileProblem({ name: 'paper.pdf', size: 10 })).toMatch(/\.tex/);
    expect(sourceFileProblem({ name: 'paper.tex', size: MAX_SOURCE_BYTES + 1 })).toMatch(/30 MB/);
    expect(sourceFileProblem({ name: 'paper.tex', size: 0 })).toBe('The file is empty.');
  });

  it('recognises bare DOIs as the server does', () => {
    expect(isDoi('10.48550/arXiv.2609.33421')).toBe(true);
    expect(isDoi('10.1000/xyz')).toBe(true);
    expect(isDoi('10.1000/')).toBe(false);
    expect(isDoi('10.1000/has whitespace')).toBe(false);
    expect(isDoi('10.1000/has\u0085whitespace')).toBe(false);
    expect(isDoi('10.1000')).toBe(false);
    expect(isDoi('2609.33421')).toBe(false);
    expect(isDoi(`10.1000/${'x'.repeat(248)}`)).toBe(true);
    expect(isDoi(`10.1000/${'x'.repeat(249)}`)).toBe(false);
    expect(isDoi(`10.1000/${'𝑥'.repeat(248)}`)).toBe(true);
  });
});

describe('buildNewPaper', () => {
  it('builds the metadata part of the upload', () => {
    expect(
      buildNewPaper({
        ...base,
        msc: '11b83, 05D10',
        doi: ' https://doi.org/10.48550/arXiv.2609.33421 ',
        openToContributors: true,
      }),
    ).toEqual({
      ok: true,
      value: {
        ai_disclosure: { level: 'assisted', statement: 'A model checked the algebra.' },
        msc: ['11B83', '05D10'],
        doi: '10.48550/arXiv.2609.33421',
        open_to_contributors: true,
      },
    });
  });

  it.each(['10.1000/xyz', ' DOI: 10.1000/xyz ', ' http://dx.doi.org/10.1000/xyz '])(
    'normalises %s before validating',
    (doi) => {
      const built = buildNewPaper({ ...base, doi });
      expect(built.ok && built.value.doi).toBe('10.1000/xyz');
    },
  );

  it.each(['', '   ', ' doi: ', 'https://doi.org/'])('sends a null DOI for %j', (doi) => {
    const built = buildNewPaper({ ...base, doi });
    expect(built.ok && built.value).toMatchObject({
      msc: [],
      doi: null,
      open_to_contributors: false,
    });
  });

  it('requires the AI disclosure and valid codes', () => {
    expect(buildNewPaper({ ...base, aiStatement: '  ' }).ok).toBe(false);
    expect(buildNewPaper({ ...base, msc: 'algebra' })).toEqual({
      ok: false,
      error: 'ALGEBRA is not an MSC 2020 code.',
    });
    expect(buildNewPaper({ ...base, doi: 'not-an-id' })).toEqual({
      ok: false,
      error: '"not-an-id" is not a DOI',
    });
    expect(buildNewPaper({ ...base, doi: '10.1000/has whitespace' }).ok).toBe(false);
  });
});
