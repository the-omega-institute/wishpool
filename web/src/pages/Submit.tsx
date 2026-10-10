import { useState, type FormEvent } from 'react';
import { useApi } from '../api/context';
import type { AiUseLevel, SubmissionKind } from '../api/types';
import { useSession } from '../auth/session';
import { Field, InlineError, Loading, SignInPrompt } from '../components/ui';
import { AI_USES, AI_USE_DESCRIPTIONS, AI_USE_LABELS, formatBytes } from '../lib/labels';
import { SOURCE_ACCEPT, buildNewPaper, sourceFileProblem } from '../lib/upload';
import { LatexText } from '../components/Markdown';
import { Link, useRouter } from '../routing/router';

export function SubmitPage() {
  const { session, person } = useSession();
  return (
    <div className="page narrow-page">
      <header className="page-head">
        <div>
          <h1>Submit your work</h1>
          <p className="lede">
            Share a paper, a short note or a conjecture. Confirm the statements, then receive a
            review and a decision.
          </p>
          <p className="small">
            <Link to={{ kind: 'policy' }}>How work is reviewed</Link>
          </p>
        </div>
      </header>
      {session.status === 'loading' ? <Loading /> : null}
      {session.status !== 'loading' && person === null ? (
        <SignInPrompt what="submit your work" />
      ) : null}
      {person ? <SubmitForm /> : null}
    </div>
  );
}

export function SubmitForm() {
  const api = useApi();
  const { person } = useSession();
  const [kind, setKind] = useState<SubmissionKind>('paper');
  const [mode, setMode] = useState<'upload' | 'type'>('upload');
  const [makePublic, setMakePublic] = useState(true);
  const [title, setTitle] = useState('');
  const [statement, setStatement] = useState('');
  const [background, setBackground] = useState('');
  const [origin, setOrigin] = useState('');
  const [authorName, setAuthorName] = useState(person?.display_name ?? '');
  const typed = kind === 'conjecture' && mode === 'type';
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
    if (!typed && file === null) {
      setError('Choose the LaTeX source file.');
      return;
    }
    if (!typed && fileProblem) {
      setError(fileProblem);
      return;
    }
    const metadata = buildNewPaper({ aiLevel, aiStatement, msc, doi, openToContributors });
    if (!metadata.ok) {
      setError(metadata.error);
      return;
    }
    metadata.value.kind = kind;
    metadata.value.make_public_after_acceptance = makePublic;
    if (typed) {
      if (!title.trim() || !statement.trim() || !authorName.trim()) {
        setError('Give a title, statement and author name.');
        return;
      }
      if (Array.from(statement).length > 20_000 || Array.from(background).length > 50_000) {
        setError('Keep the statement within 20,000 characters and background within 50,000.');
        return;
      }
      metadata.value.typed_conjecture = { title, statement, background, origin };
      metadata.value.authors = [{ name: authorName, person: person?.id }];
    }
    setBusy(true);
    api.createSubmission(metadata.value, typed ? undefined : (file ?? undefined)).then(
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
    <form className="form" onSubmit={submit} aria-label="Submit work" noValidate>
      <fieldset className="submission-choices">
        <legend>What are you sharing?</legend>
        {(['paper', 'note', 'conjecture'] as const).map((value) => (
          <label key={value} className="radio">
            <input
              type="radio"
              name="submission-kind"
              checked={kind === value}
              onChange={() => setKind(value)}
            />
            <span>
              {value === 'paper' ? 'Paper' : value === 'note' ? 'Short note' : 'Conjecture'}
            </span>
          </label>
        ))}
      </fieldset>
      {kind === 'conjecture' ? (
        <fieldset className="submission-choices">
          <legend>How would you like to enter it?</legend>
          <label className="radio">
            <input
              type="radio"
              name="conjecture-mode"
              checked={mode === 'upload'}
              onChange={() => setMode('upload')}
            />
            Upload LaTeX
          </label>
          <label className="radio">
            <input
              type="radio"
              name="conjecture-mode"
              checked={mode === 'type'}
              onChange={() => setMode('type')}
            />
            Type it
          </label>
        </fieldset>
      ) : null}
      {typed ? (
        <>
          <Field label="Title" htmlFor="conjecture-title">
            <input
              id="conjecture-title"
              value={title}
              maxLength={1000}
              onChange={(e) => setTitle(e.target.value)}
            />
          </Field>
          <Field label="Author" htmlFor="conjecture-author">
            <input
              id="conjecture-author"
              value={authorName}
              onChange={(e) => setAuthorName(e.target.value)}
            />
          </Field>
          <Field
            label="Statement"
            htmlFor="conjecture-statement"
            hint="TeX math is welcome. At most 20,000 characters."
          >
            <textarea
              id="conjecture-statement"
              rows={6}
              maxLength={20000}
              value={statement}
              onChange={(e) => setStatement(e.target.value)}
            />
          </Field>
          {statement ? (
            <div className="typed-statement-preview" aria-label="Statement preview">
              <LatexText source={statement} />
            </div>
          ) : null}
          <Field label="Background (optional)" htmlFor="conjecture-background">
            <textarea
              id="conjecture-background"
              rows={4}
              maxLength={50000}
              value={background}
              onChange={(e) => setBackground(e.target.value)}
            />
          </Field>
          <Field
            label="Origin (optional)"
            htmlFor="conjecture-origin"
            hint="My own, or a work or URL as you know it."
          >
            <input
              id="conjecture-origin"
              maxLength={2000}
              value={origin}
              onChange={(e) => setOrigin(e.target.value)}
            />
          </Field>
        </>
      ) : (
        <>
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
        </>
      )}
      <label className="checkbox">
        <input
          type="checkbox"
          checked={makePublic}
          onChange={(e) => setMakePublic(e.target.checked)}
        />
        <span>Make public after acceptance</span>
      </label>
      <p className="hint">Reviews and letters are only shown to you.</p>

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
        hint="Which systems were used and for what, or that none were. Only shown to you and staff."
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
