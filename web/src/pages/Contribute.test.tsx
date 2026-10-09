import { act, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '../api/client';
import { donor, grant } from '../test/fixtures';
import { fakeApi, renderWithApp } from '../test/render';
import { ContributePage, GrantView } from './Contribute';

describe('ContributePage', () => {
  it('shows the MCP setup for this origin and the four task kinds', async () => {
    renderWithApp(<ContributePage />, fakeApi());
    const mcp = screen.getByLabelText('MCP setup command');
    expect(mcp).toHaveTextContent(
      `claude mcp add wishpool -e WISHPOOL_URL=${window.location.origin} -e WISHPOOL_TOKEN=<NyxID access token> -- wishpool-contribute mcp`,
    );
    const cli = screen.getByLabelText('Command-line usage');
    for (const cmd of ['tasks', 'show <task>', 'lease <task>', 'submit <task> <result.json>']) {
      expect(cli).toHaveTextContent(`wishpool-contribute ${cmd}`);
    }
    expect(screen.getByText('Escape judgement')).toBeInTheDocument();
    expect(screen.getByText('Conjecture probe')).toBeInTheDocument();
    expect(screen.getByText(/records the verified Lean proof/)).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent(/trureturing/i);
    expect(await screen.findByText('Sign in to donate model quota.')).toBeInTheDocument();
  });

  it('meters the current grant and pauses it', async () => {
    const updateDonation = vi.fn(async () => ({ ...grant, status: 'paused' as const }));
    const api = fakeApi({ getDonation: vi.fn(async () => grant), updateDonation }, donor);
    renderWithApp(<ContributePage />, api);

    const meter = await screen.findByLabelText('Used this month');
    expect(meter.tagName).toBe('METER');
    expect(meter).toHaveAttribute('max', '200000');
    expect(meter).toHaveAttribute('aria-valuetext', '50,000 of 200,000 tokens');
    expect(screen.getByText('claude-opus-5-5')).toBeInTheDocument();
    expect(screen.getByText('Active')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Pause' }));
    expect(updateDonation).toHaveBeenCalledWith({ status: 'paused' });
    expect(await screen.findByRole('button', { name: 'Resume' })).toBeInTheDocument();
    expect(screen.getByText('Paused')).toBeInTheDocument();
  });

  it('offers the donate form when there is no grant', async () => {
    const api = fakeApi({ getDonation: vi.fn(async () => null) }, donor);
    renderWithApp(<ContributePage />, api);
    expect(await screen.findByRole('form', { name: 'Donate quota' })).toBeInTheDocument();
    expect(screen.getByLabelText('Monthly cap (tokens)')).toHaveValue('200000');
  });

  it('says when donations are not enabled', async () => {
    const api = fakeApi(
      {
        getDonation: vi.fn(async () => {
          throw new ApiError({ status: 403, code: 'forbidden', detail: 'disabled' });
        }),
      },
      donor,
    );
    renderWithApp(<ContributePage />, api);
    expect(
      await screen.findByText('Donations are not enabled on this deployment.'),
    ).toBeInTheDocument();
  });
});

describe('GrantView', () => {
  it('shows zero used when the counter is from an earlier month', async () => {
    renderWithApp(
      <GrantView
        grant={{ ...grant, period: '2026-09', used: 190_000 }}
        onChange={() => undefined}
        now={new Date('2026-10-08T00:00:00Z')}
      />,
      fakeApi(),
    );
    expect(screen.getByLabelText('Used this month')).toHaveAttribute(
      'aria-valuetext',
      '0 of 200,000 tokens',
    );
    expect(screen.getByText(/last metered use was in 2026-09/)).toBeInTheDocument();
    // Let the session and policy providers settle inside the test.
    expect(await screen.findByRole('button', { name: 'Pause' })).toBeInTheDocument();
    await act(async () => undefined);
  });
});
