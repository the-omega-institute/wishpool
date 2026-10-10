import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '../api/client';
import { author, draft } from '../test/fixtures';
import { fakeApi, renderWithApp } from '../test/render';
import { SubmitPage } from './Submit';

describe('SubmitPage', () => {
  it('asks anonymous visitors to sign in', async () => {
    renderWithApp(<SubmitPage />, fakeApi());
    expect(await screen.findByText('Sign in to submit your work.')).toBeInTheDocument();
  });

  it('uploads the source with the metadata and opens the draft', async () => {
    const createSubmission = vi.fn(async () => draft);
    renderWithApp(<SubmitPage />, fakeApi({ createSubmission }, author));
    const user = userEvent.setup();

    const file = new File(['\\documentclass{amsart}'], 'paper.tar.gz', {
      type: 'application/gzip',
    });
    await user.upload(await screen.findByLabelText('LaTeX source'), file);
    expect(
      screen.getByText('If the paper already has one, e.g. 10.48550/arXiv.2609.33421'),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText('arXiv identifier (optional)')).toBeNull();
    await user.click(screen.getByRole('radio', { name: /^Substantial/ }));
    await user.type(screen.getByLabelText('AI disclosure statement'), 'A model drafted Lemma 2.');
    await user.type(screen.getByLabelText('MSC 2020 codes (optional)'), '11b83 05d10');
    await user.type(
      screen.getByLabelText('DOI (optional)'),
      'https://doi.org/10.48550/arXiv.2609.33421',
    );
    await user.click(screen.getByRole('checkbox', { name: /volunteer contributors/ }));
    await user.click(screen.getByRole('button', { name: 'Upload and read statements' }));

    expect(createSubmission).toHaveBeenCalledWith(
      {
        kind: 'paper',
        make_public_after_acceptance: true,
        ai_disclosure: { level: 'substantial', statement: 'A model drafted Lemma 2.' },
        msc: ['11B83', '05D10'],
        doi: '10.48550/arXiv.2609.33421',
        open_to_contributors: true,
      },
      file,
    );
    await waitFor(() => expect(window.location.pathname).toBe('/submissions/sub-draft'));
  });

  it('rejects an invalid DOI before uploading', async () => {
    const createSubmission = vi.fn(async () => draft);
    renderWithApp(<SubmitPage />, fakeApi({ createSubmission }, author));
    const user = userEvent.setup();
    await user.upload(
      await screen.findByLabelText('LaTeX source'),
      new File(['x'], 'paper.tex', { type: 'application/x-tex' }),
    );
    await user.type(screen.getByLabelText('AI disclosure statement'), 'None.');
    await user.type(screen.getByLabelText('DOI (optional)'), '2609.33421');
    await user.click(screen.getByRole('button', { name: 'Upload and read statements' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('"2609.33421" is not a DOI');
    expect(createSubmission).not.toHaveBeenCalled();
  });

  it('refuses a PDF before uploading and shows the server’s detail on failure', async () => {
    const createSubmission = vi.fn(async () => {
      throw new ApiError({
        status: 422,
        code: 'invalid',
        detail: 'no main file: the archive has no .tex file with \\documentclass',
      });
    });
    renderWithApp(<SubmitPage />, fakeApi({ createSubmission }, author));
    const user = userEvent.setup({ applyAccept: false });
    const input = await screen.findByLabelText('LaTeX source');

    await user.upload(input, new File(['%PDF'], 'paper.pdf', { type: 'application/pdf' }));
    await user.type(screen.getByLabelText('AI disclosure statement'), 'None.');
    await user.click(screen.getByRole('button', { name: 'Upload and read statements' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(/Choose a \.tex file/);
    expect(createSubmission).not.toHaveBeenCalled();

    await user.upload(input, new File(['x'], 'paper.zip', { type: 'application/zip' }));
    await user.click(screen.getByRole('button', { name: 'Upload and read statements' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'no main file: the archive has no .tex file with \\documentclass',
    );
  });
  it('uses the same upload for a short note with the publication checkbox off', async () => {
    const createSubmission = vi.fn(async () => draft);
    renderWithApp(<SubmitPage />, fakeApi({ createSubmission }, author));
    const user = userEvent.setup();
    await user.click(await screen.findByRole('radio', { name: 'Short note' }));
    const publication = screen.getByRole('checkbox', { name: 'Make public after acceptance' });
    expect(publication).toBeChecked();
    await user.click(publication);
    const source = new File(['source'], 'note.tex', { type: 'application/x-tex' });
    await user.upload(screen.getByLabelText('LaTeX source'), source);
    await user.type(screen.getByLabelText('AI disclosure statement'), 'No AI was used.');
    await user.click(screen.getByRole('button', { name: 'Upload and read statements' }));
    expect(createSubmission).toHaveBeenCalledWith(
      expect.objectContaining({ kind: 'note', make_public_after_acceptance: false }),
      source,
    );
  });

  it('submits typed conjectures with TeX preview through the same endpoint without a file', async () => {
    const createSubmission = vi.fn(async () => draft);
    renderWithApp(<SubmitPage />, fakeApi({ createSubmission }, author));
    const user = userEvent.setup();
    await user.click(await screen.findByRole('radio', { name: 'Conjecture' }));
    await user.click(screen.getByRole('radio', { name: 'Type it' }));
    expect(screen.queryByLabelText('LaTeX source')).not.toBeInTheDocument();
    await user.type(screen.getByLabelText('Title'), 'An open bound');
    await user.type(screen.getByLabelText('Statement'), 'Every $n > 1$ has the property.');
    await user.type(screen.getByLabelText('Background (optional)'), 'Let $n$ be a natural number.');
    await user.type(screen.getByLabelText('Origin (optional)'), 'my own');
    await user.type(screen.getByLabelText('AI disclosure statement'), 'No AI was used.');
    const preview = screen.getByLabelText('Statement preview');
    expect(preview.querySelector('.katex')).not.toBeNull();
    expect(screen.getByRole('checkbox', { name: 'Make public after acceptance' })).toBeChecked();
    expect(screen.getByText('Reviews and letters are only shown to you.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Upload and read statements' }));
    expect(createSubmission).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: 'conjecture',
        make_public_after_acceptance: true,
        typed_conjecture: {
          title: 'An open bound',
          statement: 'Every $n > 1$ has the property.',
          background: 'Let $n$ be a natural number.',
          origin: 'my own',
        },
        authors: [{ name: author.display_name, person: author.id }],
      }),
      undefined,
    );
  });
});
