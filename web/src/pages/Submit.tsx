import { useState, type FormEvent } from 'react';
import { useApi } from '../api/context';
import type { AiUseLevel } from '../api/types';
import { useSession } from '../auth/session';
import { Field, InlineError, Loading, SignInPrompt } from '../components/ui';
import { AI_USES, AI_USE_DESCRIPTIONS, AI_USE_LABELS, formatBytes } from '../lib/labels';
import { SOURCE_ACCEPT, buildNewPaper, sourceFileProblem } from '../lib/upload';
import { Link, useRouter } from '../routing/router';

export function SubmitPage() {
  const { session, person } = useSession();
  return (
    <div className="page narrow-page">
      <header className="page-head">
        <div>
          <h1>Submit a paper</h1>
          <p className="lede">
            Upload the LaTeX source of your own paper. The server reads the title, authors, abstract
            and every theorem-like statement and compiles the PDF. You then confirm the statements
            before review begins.
          </p>
          <p className="small">
            <Link to={{ kind: 'policy' }}>How papers are reviewed</Link>
          </p>
        </div>
      </header>
      {session.status === 'loading' ? <Loading /> : null}
      {session.status !== 'loading' && person === null ? (
        <SignInPrompt what="submit a paper" />
      ) : null}
      {person ? <SubmitForm /> : null}
    </div>
  );
}

export function SubmitForm() {
  const api = useApi();
  const { navigate } = useRouter();
  const [file, setFile] = useState<File | null>(null);
  const [aiLevel, setAiLevel] = useState<AiUseLevel>('none');
  const [aiStatement, setAiStatement] = useState('');
  const [msc, setMsc] = useState('');
  const [doi, setDoi] = useState('');
  const [openToContributors, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);

  const fileProblem = file ? sourceFileProblem(file) : null;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setError(null);
    if (file === null) {
      setError('Choose the LaTeX source file.');
      return;
    }
    if (fileProblem) {
      setError(fileProblem);
      return;
    }
    const metadata = buildNewPaper({ aiLevel, aiStatement, msc, doi, openToContributors });
    if (!metadata.ok) {
      setError(metadata.error);
      return;
    }
    setBusy(true);
    api.createSubmission(metadata.value, file).then(
      (s) => {
        setBusy(false);
        navigate({ kind: 'submission', id: s.id });
      },
      (e: unknown) => {
        setBusy(false);
        setError(e);
      },
    );
  };

  return (
    <form className="form" onSubmit={submit} aria-label="Submit a paper" noValidate>
      <Field
        label="LaTeX source"
        htmlFor="source-file"
        hint="A single .tex file, or a .zip or .tar.gz archive with the main file, bibliography and figures. At most 30 MB."
      >
        <input
          id="source-file"
          type="file"
          accept={SOURCE_ACCEPT}
          required
          onChange={(e) => setFile(e.target.files?.[0] ?? null)}
        />
      </Field>
      {file ? (
        <p className="small" aria-live="polite">
          {file.name} · {formatBytes(file.size)}
          {fileProblem ? <span className="form-error"> — {fileProblem}</span> : null}
        </p>
      ) : null}

      <fieldset className="radio-list">
        <legend>Use of AI</legend>
        {AI_USES.map((level) => (
          <label key={level} className="radio">
            <input
              type="radio"
              name="ai-level"
              value={level}
              checked={aiLevel === level}
              onChange={() => setAiLevel(level)}
            />
            <span>
              <strong>{AI_USE_LABELS[level]}</strong> — {AI_USE_DESCRIPTIONS[level]}
            </span>
          </label>
        ))}
      </fieldset>
      <Field
        label="AI disclosure statement"
        htmlFor="ai-statement"
        hint="Which systems were used and for what, or that none were. Shown on the public page if the paper is accepted."
      >
        <textarea
          id="ai-statement"
          rows={3}
          required
          value={aiStatement}
          onChange={(e) => setAiStatement(e.target.value)}
        />
      </Field>

      <div className="form-row">
        <Field label="MSC 2020 codes (optional)" htmlFor="msc" hint="e.g. 11B83, 05D10">
          <input id="msc" value={msc} onChange={(e) => setMsc(e.target.value)} />
        </Field>
        <Field
          label="DOI (optional)"
          htmlFor="doi"
          hint="If the paper already has one, e.g. 10.48550/arXiv.2609.33421"
        >
          <input id="doi" value={doi} onChange={(e) => setDoi(e.target.value)} />
        </Field>
      </div>

      <label className="checkbox">
        <input
          type="checkbox"
          checked={openToContributors}
          onChange={(e) => setOpen(e.target.checked)}
        />
        <span>
          Let volunteer contributors help analyse the paper. They will see the statements, the title
          and the abstract while the paper is in review. You can change this later.
        </span>
      </label>

      <div className="button-row">
        <button type="submit" className="button" disabled={busy}>
          {busy ? 'Uploading…' : 'Upload and read statements'}
        </button>
      </div>
      <InlineError error={error} />
    </form>
  );
}
