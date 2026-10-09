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
    expect(await screen.findByText('Sign in to submit a paper.')).toBeInTheDocument();
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
});
