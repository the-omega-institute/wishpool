import { describe, expect, it } from 'vitest';
import {
  artifactLabel,
  commitHref,
  orcidHref,
  safeHttpUrl,
  sourceHref,
  sourceLabel,
} from './links';
import type { Source } from '../api/types';

const href = (kind: Source['kind'], locator: string) => sourceHref({ kind, locator });

describe('sourceHref (deposit links)', () => {
  it('links arXiv identifiers to the abstract page', () => {
    expect(href('arxiv', '2401.01234')).toBe('https://arxiv.org/abs/2401.01234');
    expect(href('arxiv', '2401.01234v2')).toBe('https://arxiv.org/abs/2401.01234v2');
    expect(href('arxiv', 'arXiv:2401.01234')).toBe('https://arxiv.org/abs/2401.01234');
    expect(href('arxiv', 'math/0211159')).toBe('https://arxiv.org/abs/math/0211159');
    expect(href('arxiv', 'https://arxiv.org/pdf/2401.01234.pdf')).toBe(
      'https://arxiv.org/abs/2401.01234',
    );
  });

  it('resolves DOIs through doi.org', () => {
    expect(href('doi', '10.1000/xyz123')).toBe('https://doi.org/10.1000/xyz123');
    expect(href('doi', 'doi:10.1000/xyz123')).toBe('https://doi.org/10.1000/xyz123');
    expect(href('doi', 'https://doi.org/10.1000/xyz123')).toBe('https://doi.org/10.1000/xyz123');
    expect(href('doi', '10.1000/a?b#c')).toBe('https://doi.org/10.1000/a%3Fb%23c');
  });

  it('resolves Zenodo DOIs like DOIs, and bare record numbers to zenodo.org', () => {
    expect(href('zenodo', '10.5281/zenodo.1234567')).toBe('https://doi.org/10.5281/zenodo.1234567');
    expect(href('zenodo', '1234567')).toBe('https://zenodo.org/records/1234567');
    expect(href('zenodo', 'https://zenodo.org/records/1')).toBe('https://zenodo.org/records/1');
  });

  it('passes http(s) URLs through as-is and refuses other schemes', () => {
    expect(href('url', 'https://example.org/paper.pdf')).toBe('https://example.org/paper.pdf');
    expect(href('hexagon', 'https://hexagon.example/p/42')).toBe('https://hexagon.example/p/42');
    expect(href('url', 'javascript:alert(1)')).toBeNull();
    expect(href('url', 'not a url')).toBeNull();
    expect(href('hexagon', 'hx-42')).toBeNull();
  });

  it('links OEIS sequences and never links personal communication', () => {
    expect(href('oeis', 'a000045')).toBe('https://oeis.org/A000045');
    expect(href('personal', 'Erdős, 1990')).toBeNull();
    expect(href('arxiv', '   ')).toBeNull();
  });
});

describe('sourceLabel', () => {
  it('prints citation forms', () => {
    expect(sourceLabel({ kind: 'arxiv', locator: 'arXiv:2401.01234' })).toBe('arXiv:2401.01234');
    expect(sourceLabel({ kind: 'doi', locator: '10.1/x', year: 1999 })).toBe('doi:10.1/x (1999)');
    expect(sourceLabel({ kind: 'oeis', locator: 'a000045' })).toBe('OEIS A000045');
    expect(sourceLabel({ kind: 'personal', locator: '' })).toBe('Personal communication');
  });
});

describe('formal artifact links', () => {
  const commit = 'abcdef0123456789abcdef0123456789abcdef01';
  it('pins forge repositories to the commit', () => {
    expect(commitHref({ repository: 'https://github.com/o/r.git', commit, declarations: [] })).toBe(
      `https://github.com/o/r/tree/${commit}`,
    );
    expect(artifactLabel({ repository: 'https://github.com/o/r', commit, declarations: [] })).toBe(
      'o/r@abcdef012345',
    );
  });
  it('links unknown hosts to the repository and refuses non-http', () => {
    expect(commitHref({ repository: 'https://git.example/r', commit, declarations: [] })).toBe(
      'https://git.example/r',
    );
    expect(commitHref({ repository: 'git@github.com:o/r', commit, declarations: [] })).toBeNull();
  });
});

describe('misc links', () => {
  it('validates ORCID iDs', () => {
    expect(orcidHref('0000-0002-1825-0097')).toBe('https://orcid.org/0000-0002-1825-0097');
    expect(orcidHref('https://orcid.org/0000-0002-1694-233X')).toBe(
      'https://orcid.org/0000-0002-1694-233X',
    );
    expect(orcidHref('1234')).toBeNull();
  });
  it('accepts only http(s)', () => {
    expect(safeHttpUrl('data:text/html,hi')).toBeNull();
    expect(safeHttpUrl(' http://a.b/ ')).toBe('http://a.b/');
  });
});
