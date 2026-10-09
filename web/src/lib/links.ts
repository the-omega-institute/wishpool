import type { FormalArtifact, Source } from '../api/types';

/** Only http(s) URLs are ever rendered as links; anything else is shown as text. */
export function safeHttpUrl(value: string): string | null {
  const trimmed = value.trim();
  try {
    const url = new URL(trimmed);
    return url.protocol === 'https:' || url.protocol === 'http:' ? url.href : null;
  } catch {
    return null;
  }
}

const DOI_PREFIX = /^(?:https?:\/\/(?:dx\.)?doi\.org\/|doi:\s*)/i;
const ARXIV_PREFIX = /^(?:https?:\/\/arxiv\.org\/(?:abs|pdf)\/|arxiv:\s*)/i;

/** A bare DOI (`10.xxxx/...`), with any `doi:` or resolver prefix removed. */
export function normaliseDoi(locator: string): string {
  return locator.trim().replace(DOI_PREFIX, '').trim();
}

/** A bare arXiv identifier, e.g. `2401.01234v2` or `math/0211159`. */
export function normaliseArxiv(locator: string): string {
  return locator
    .trim()
    .replace(ARXIV_PREFIX, '')
    .replace(/\.pdf$/i, '');
}

function doiHref(doi: string): string {
  // DOIs may contain characters with URL meaning; keep `/` readable.
  return `https://doi.org/${encodeURI(doi).replace(/[?#]/g, encodeURIComponent)}`;
}

/**
 * Where a deposit or source can be read. `null` when the locator is not
 * resolvable to a public http(s) address (personal communication, an
 * unrecognised Hexagon locator, a malformed URL).
 */
export function sourceHref(source: Source): string | null {
  const locator = source.locator.trim();
  if (locator === '') return null;
  switch (source.kind) {
    case 'arxiv': {
      const id = normaliseArxiv(locator);
      return id === '' ? null : `https://arxiv.org/abs/${encodeURI(id)}`;
    }
    case 'doi': {
      const doi = normaliseDoi(locator);
      return doi.startsWith('10.') ? doiHref(doi) : safeHttpUrl(locator);
    }
    case 'zenodo': {
      const doi = normaliseDoi(locator);
      if (doi.startsWith('10.')) return doiHref(doi);
      if (/^\d+$/.test(locator)) return `https://zenodo.org/records/${locator}`;
      return safeHttpUrl(locator);
    }
    case 'oeis': {
      const id = locator.toUpperCase();
      return /^A\d{6,}$/.test(id) ? `https://oeis.org/${id}` : safeHttpUrl(locator);
    }
    case 'hexagon':
    case 'url':
      return safeHttpUrl(locator);
    case 'personal':
      return null;
  }
}

/** A compact citation form of a source, e.g. `arXiv:2401.01234` or `doi:10.1/x`. */
export function sourceLabel(source: Source): string {
  const locator = source.locator.trim();
  let label: string;
  switch (source.kind) {
    case 'arxiv':
      label = `arXiv:${normaliseArxiv(locator)}`;
      break;
    case 'doi':
      label = locator.startsWith('http') ? locator : `doi:${normaliseDoi(locator)}`;
      break;
    case 'zenodo':
      label = /^\d+$/.test(locator) ? `Zenodo ${locator}` : `Zenodo ${normaliseDoi(locator)}`;
      break;
    case 'oeis':
      label = `OEIS ${locator.toUpperCase()}`;
      break;
    case 'hexagon':
      label = `Hexagon ${locator}`;
      break;
    case 'personal':
      label = `Personal communication${locator ? `: ${locator}` : ''}`;
      break;
    case 'url':
      label = locator;
      break;
  }
  return source.year === undefined || source.year === null ? label : `${label} (${source.year})`;
}

const FORGES = /^https?:\/\/(?:www\.)?(?:github\.com|gitlab\.com|codeberg\.org)\//i;

/** A link to the repository at the pinned commit, when the forge is known. */
export function commitHref(artifact: FormalArtifact): string | null {
  const repo = safeHttpUrl(artifact.repository.trim().replace(/\.git$/, ''));
  if (repo === null) return null;
  const base = repo.replace(/\/+$/, '');
  return FORGES.test(base) ? `${base}/tree/${encodeURIComponent(artifact.commit)}` : base;
}

/** `owner/repo@abcdef1` for display. */
export function artifactLabel(artifact: FormalArtifact): string {
  const repo = artifact.repository
    .trim()
    .replace(/\.git$/, '')
    .replace(/\/+$/, '')
    .replace(/^https?:\/\/(?:www\.)?(?:github\.com|gitlab\.com|codeberg\.org)\//i, '');
  return `${repo}@${artifact.commit.slice(0, 12)}`;
}

/** ORCID iD link; only for well-formed identifiers. */
export function orcidHref(orcid: string): string | null {
  const id = orcid.trim().replace(/^https?:\/\/orcid\.org\//i, '');
  return /^\d{4}-\d{4}-\d{4}-\d{3}[\dX]$/.test(id) ? `https://orcid.org/${id}` : null;
}
