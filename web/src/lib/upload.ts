import type { AiUseLevel, NewPaper } from '../api/types';
import { normaliseDoi } from './links';
import { parseMsc } from './labels';
import type { Parsed } from './parsed';

/** The server's upload limit. */
export const MAX_SOURCE_BYTES = 30 * 1024 * 1024;
export const SOURCE_EXTENSIONS = ['.tex', '.zip', '.tar.gz'] as const;
/** For the file picker's `accept`. */
export const SOURCE_ACCEPT =
  '.tex,.zip,.tar.gz,.tgz,application/zip,application/gzip,application/x-tex';

/** Why a chosen file cannot be uploaded, or null when it can. */
export function sourceFileProblem(file: Pick<File, 'name' | 'size'>): string | null {
  const name = file.name.toLowerCase();
  const known = SOURCE_EXTENSIONS.some((ext) => name.endsWith(ext)) || name.endsWith('.tgz');
  if (!known) return 'Choose a .tex file, or a .zip or .tar.gz archive of the LaTeX source.';
  if (file.size === 0) return 'The file is empty.';
  if (file.size > MAX_SOURCE_BYTES) return 'The source must be at most 30 MB.';
  return null;
}

/** Bare DOIs accepted by Layer 2: at most 256 characters and no whitespace. */
export function isDoi(value: string): boolean {
  const slash = value.indexOf('/');
  return (
    Array.from(value).length <= 256 &&
    value.startsWith('10.') &&
    slash >= 0 &&
    slash < value.length - 1 &&
    !/\p{White_Space}/u.test(value)
  );
}

/** MSC 2020 codes the server accepts, e.g. `11B83`, `05C`, `11-02`. */
export function isMscCode(code: string): boolean {
  return /^\d{2}[A-Za-z0-9-]{0,3}$/.test(code);
}

export interface PaperFields {
  aiLevel: AiUseLevel;
  aiStatement: string;
  msc: string;
  doi: string;
  openToContributors: boolean;
}

/** The `metadata` part of an upload, from the submit form. */
export function buildNewPaper(fields: PaperFields): Parsed<NewPaper> {
  const statement = fields.aiStatement.trim();
  if (statement === '') {
    return { ok: false, error: 'Describe how AI was used, or state that it was not.' };
  }
  const msc = parseMsc(fields.msc);
  const badMsc = msc.find((c) => !isMscCode(c));
  if (badMsc !== undefined) return { ok: false, error: `${badMsc} is not an MSC 2020 code.` };
  if (msc.length > 8) return { ok: false, error: 'Give at most 8 MSC codes.' };
  const doi = normaliseDoi(fields.doi) || null;
  if (doi !== null && !isDoi(doi)) {
    return { ok: false, error: `"${doi}" is not a DOI` };
  }
  return {
    ok: true,
    value: {
      ai_disclosure: { level: fields.aiLevel, statement },
      msc,
      doi,
      open_to_contributors: fields.openToContributors,
    },
  };
}
