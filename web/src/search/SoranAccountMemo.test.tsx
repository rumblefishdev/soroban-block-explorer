import { act, screen, waitFor } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { Account, MuxedAccount } from '@stellar/stellar-sdk';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../test-utils.js';
import { SoranAccountMemo } from './SoranAccountMemo.js';
import {
  resolveSoranPayment,
  resolveSoranPrimary,
  type SoranMemo,
  type SoranResolve,
} from './soran.js';

const current = vi.hoisted(() => ({
  network: 'testnet' as 'mainnet' | 'testnet',
}));

vi.mock('../network.js', () => ({
  get network() {
    return current.network;
  },
}));

vi.mock('./soran.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./soran.js')>();
  return {
    ...actual,
    resolveSoranPayment: vi.fn(),
    resolveSoranPrimary: vi.fn(),
  };
});

const ACCOUNT = 'GC526FUILJ6NLFXKCOOGTMDXNRW7MYSEK2UNRJV5FYWOGYDE4LOKXFEM';
const OTHER_ACCOUNT =
  'GA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVSGZ';
const MUXED = new MuxedAccount(new Account(ACCOUNT, '0'), '77').accountId();
const COPY_LABEL = 'Copy required Soran memo';
const payment = vi.mocked(resolveSoranPayment);
const primary = vi.mocked(resolveSoranPrimary);

function result(
  address = ACCOUNT,
  memo: SoranMemo = { type: 'id', value: '77' }
): SoranResolve {
  return { kind: 'resolved', payment: { address, memo } };
}

function renderMemo(name: string | null = 'alice.nova', address = ACCOUNT) {
  const suffix = name === null ? '' : `?soran=${encodeURIComponent(name)}`;
  return renderWithProviders(<SoranAccountMemo address={address} />, {
    initialEntries: [`/accounts/${address}${suffix}`],
  });
}

beforeEach(() => {
  current.network = 'testnet';
  payment.mockReset().mockResolvedValue(result());
  primary.mockReset().mockResolvedValue(null);
});

describe('Soran memo on an account page', () => {
  it('resolves the name in the URL on arrival and again after a refresh', async () => {
    const first = renderMemo();
    expect(
      await screen.findByText('Required memo (id): 77')
    ).toBeInTheDocument();
    expect(screen.getByText('Memo')).toBeInTheDocument();
    expect(screen.getByText('alice.nova')).toBeInTheDocument();
    expect(payment).toHaveBeenCalledWith(
      'alice.nova',
      'testnet',
      expect.any(AbortSignal)
    );
    expect(primary).not.toHaveBeenCalled();

    first.unmount();
    renderMemo();

    expect(
      await screen.findByText('Required memo (id): 77')
    ).toBeInTheDocument();
    expect(payment).toHaveBeenCalledTimes(2);
    expect(primary).not.toHaveBeenCalled();
  });

  it('uses the account Primary name when the address was opened directly', async () => {
    primary.mockResolvedValue('alice.nova');
    renderMemo(null);

    expect(
      await screen.findByText('Required memo (id): 77')
    ).toBeInTheDocument();
    expect(primary).toHaveBeenCalledWith(
      ACCOUNT,
      'testnet',
      expect.any(AbortSignal)
    );
    expect(payment).toHaveBeenCalledWith(
      'alice.nova',
      'testnet',
      expect.any(AbortSignal)
    );
  });

  it('uses the explicit alias rather than replacing it with the account Primary', async () => {
    primary.mockResolvedValue('primary.nova');
    renderMemo('alias.nova');

    expect(
      await screen.findByText('Required memo (id): 77')
    ).toBeInTheDocument();
    expect(screen.getByText('alias.nova')).toBeInTheDocument();
    expect(payment).toHaveBeenCalledWith(
      'alias.nova',
      'testnet',
      expect.any(AbortSignal)
    );
    expect(primary).not.toHaveBeenCalled();
  });

  it('does not resolve a payment when the account has no Primary name', async () => {
    renderMemo(null);

    await waitFor(() => {
      expect(primary).toHaveBeenCalledTimes(1);
      expect(screen.queryByText('Loading memo…')).not.toBeInTheDocument();
    });
    expect(payment).not.toHaveBeenCalled();
    expect(screen.queryByText(/Required memo/)).not.toBeInTheDocument();
  });

  it.each([OTHER_ACCOUNT, MUXED])(
    'withholds the memo when the name resolves to a different full address: %s',
    async (destination) => {
      payment.mockResolvedValue(
        destination === MUXED
          ? result(destination, { type: 'none' })
          : result(destination)
      );
      renderMemo();

      expect(
        await screen.findByText('This name no longer resolves to this account.')
      ).toBeInTheDocument();
      expect(screen.queryByText(/Required memo/)).not.toBeInTheDocument();
      expect(
        screen.queryByText('No memo required for this name.')
      ).not.toBeInTheDocument();
      expect(
        screen.queryByRole('button', { name: COPY_LABEL })
      ).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Retry memo' })).toBeEnabled();
    }
  );

  it('states explicitly when the verified name requires no memo', async () => {
    payment.mockResolvedValue(result(ACCOUNT, { type: 'none' }));
    renderMemo();

    expect(
      await screen.findByText('No memo required for this name.')
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: COPY_LABEL })
    ).not.toBeInTheDocument();
  });

  it.each([
    { type: 'id', value: '18446744073709551615' },
    { type: 'text', value: 'invoice 77' },
    { type: 'hash', value: 'ab'.repeat(32) },
  ] as const)('displays and copies the complete $type memo', async (memo) => {
    const user = userEvent.setup();
    payment.mockResolvedValue(result(ACCOUNT, memo));
    renderMemo();

    expect(
      await screen.findByText(`Required memo (${memo.type}): ${memo.value}`)
    ).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: COPY_LABEL }));

    expect(await navigator.clipboard.readText()).toBe(memo.value);
  });

  it.each(['failed result', 'rejected request'])(
    'retries a payment lookup after a %s',
    async (failure) => {
      const user = userEvent.setup();
      if (failure === 'failed result') {
        payment.mockResolvedValueOnce({
          kind: 'failed',
          reason: 'Soran is temporarily unavailable.',
        });
      } else {
        payment.mockRejectedValueOnce(new Error('RPC unavailable'));
      }
      renderMemo();

      const retry = await screen.findByRole('button', {
        name: 'Retry memo',
      });
      expect(payment).toHaveBeenCalledTimes(1);
      expect(screen.queryByText(/Required memo/)).not.toBeInTheDocument();
      await user.click(retry);

      expect(
        await screen.findByText('Required memo (id): 77')
      ).toBeInTheDocument();
      expect(payment).toHaveBeenCalledTimes(2);
    }
  );

  it('withholds a late answer for an account that is no longer displayed', async () => {
    let finishOld: (value: SoranResolve) => void = () => undefined;
    let finishNext: (value: SoranResolve) => void = () => undefined;
    const oldPayment = new Promise<SoranResolve>((resolve) => {
      finishOld = resolve;
    });
    const nextPayment = new Promise<SoranResolve>((resolve) => {
      finishNext = resolve;
    });
    primary
      .mockResolvedValueOnce('alice.nova')
      .mockResolvedValueOnce('bob.nova');
    payment.mockReturnValueOnce(oldPayment).mockReturnValueOnce(nextPayment);
    const { rerender } = renderMemo(null);
    await waitFor(() => expect(payment).toHaveBeenCalledTimes(1));

    rerender(<SoranAccountMemo address={OTHER_ACCOUNT} />);
    await waitFor(() => expect(payment).toHaveBeenCalledTimes(2));
    await act(async () => finishOld(result()));

    expect(screen.getByText('Loading memo…')).toBeInTheDocument();
    expect(screen.queryByText('alice.nova')).not.toBeInTheDocument();
    expect(screen.queryByText(/Required memo/)).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: COPY_LABEL })
    ).not.toBeInTheDocument();

    await act(async () =>
      finishNext(result(OTHER_ACCOUNT, { type: 'id', value: '88' }))
    );
    expect(
      await screen.findByText('Required memo (id): 88')
    ).toBeInTheDocument();
    expect(
      screen.queryByText('Required memo (id): 77')
    ).not.toBeInTheDocument();
  });

  it('removes an already displayed memo immediately when the current account changes', async () => {
    primary
      .mockResolvedValueOnce('alice.nova')
      .mockReturnValueOnce(new Promise(() => undefined));
    const { rerender } = renderMemo(null);
    expect(
      await screen.findByText('Required memo (id): 77')
    ).toBeInTheDocument();

    rerender(<SoranAccountMemo address={OTHER_ACCOUNT} />);

    expect(screen.queryByText(/Required memo/)).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: COPY_LABEL })
    ).not.toBeInTheDocument();
    expect(screen.queryByText('alice.nova')).not.toBeInTheDocument();
  });

  it.each(['bad!.nova', 'alice..nova', ''])(
    'rejects an invalid explicit name without falling back to Primary: %j',
    async (name) => {
      primary.mockResolvedValue('alice.nova');
      renderMemo(name);
      await act(async () => undefined);

      expect(screen.getByText('Invalid Soran name.')).toBeInTheDocument();
      expect(payment).not.toHaveBeenCalled();
      expect(primary).not.toHaveBeenCalled();
    }
  );

  it.each([null, 'alice.nova'])(
    'renders nothing and makes no requests on mainnet with name %j',
    async (name) => {
      current.network = 'mainnet';
      const { container } = renderMemo(name);
      await act(async () => undefined);

      expect(container).toBeEmptyDOMElement();
      expect(payment).not.toHaveBeenCalled();
      expect(primary).not.toHaveBeenCalled();
    }
  );
});
