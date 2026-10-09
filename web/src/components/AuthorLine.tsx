import type { Author } from '../api/types';
import { orcidHref } from '../lib/links';
import { ExternalLink } from './ui';

export function AuthorLine({ authors }: { authors: readonly Author[] }) {
  return (
    <ul className="author-line">
      {authors.map((a, i) => {
        const orcid = a.orcid ? orcidHref(a.orcid) : null;
        return (
          <li key={`${a.name}-${i}`}>
            <span className="author-name">{a.name}</span>
            {orcid ? (
              <>
                {' '}
                <ExternalLink href={orcid}>
                  <span className="orcid" aria-label={`ORCID iD of ${a.name}`}>
                    iD
                  </span>
                </ExternalLink>
              </>
            ) : null}
            {a.affiliation ? <span className="affiliation">{a.affiliation}</span> : null}
          </li>
        );
      })}
    </ul>
  );
}
