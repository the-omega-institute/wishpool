import type { PaperSummary } from '../api/types';
import { formatDate } from './format';
import { formatAuthors } from './labels';
import { sourceHref } from './links';

/** A plain-text citation for an accepted paper's public record. */
export function paperCitation(paper: PaperSummary, origin: string): string {
  const authors = formatAuthors(paper.authors);
  const title = paper.title.trim().replace(/\.$/, '');
  const doiHref = paper.doi ? sourceHref({ kind: 'doi', locator: paper.doi }) : null;
  return [
    `${authors}. ${title}.`,
    `Wishpool record ${paper.record}${paper.accepted_at ? `, accepted ${formatDate(paper.accepted_at)}` : ''}.`,
    paper.doi ? `doi:${paper.doi}${doiHref ? ` (${doiHref})` : ''}.` : null,
    `${origin}/papers/${encodeURIComponent(paper.record)}`,
  ]
    .filter((part) => part !== null)
    .join(' ');
}
