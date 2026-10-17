import { act, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../test-utils.js';
import { SoranPrimaryName } from './SoranPrimaryName.js';
import { resolveSoranPrimary } from './soran.js';

const current = vi.hoisted(() => ({
  network: 'testnet' as 'mainnet' | 'testnet',
}));

vi.mock('../network.js', () => ({
  get network() {
    return current.network;
  },
}));

vi.mock('./soran.js', () => ({ resolveSoranPrimary: vi.fn() }));

const resolve = vi.mocked(resolveSoranPrimary);
const ACCOUNT = 'GC526FUILJ6NLFXKCOOGTMDXNRW7MYSEK2UNRJV5FYWOGYDE4LOKXFEM';
const CONTRACT = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM';

beforeEach(() => {
  current.network = 'testnet';
  resolve.mockReset();
});

describe('SoranPrimaryName', () => {
  it('makes no request on mainnet', async () => {
    current.network = 'mainnet';
    const { container } = renderWithProviders(
      <SoranPrimaryName address={ACCOUNT} />
    );

    await act(async () => undefined);

    expect(resolve).not.toHaveBeenCalled();
    expect(container).toBeEmptyDOMElement();
  });

  it.each([ACCOUNT, CONTRACT])(
    'shows the onchain Soran Primary for a testnet address: %s',
    async (address) => {
      resolve.mockResolvedValue('alice.nova');
      renderWithProviders(<SoranPrimaryName address={address} />);

      expect(await screen.findByText('alice.nova')).toHaveAttribute(
        'title',
        "Primary name read from Soran's Testnet contracts"
      );
      expect(resolve).toHaveBeenCalledWith(
        address,
        'testnet',
        expect.any(AbortSignal)
      );
    }
  );

  it('renders nothing when the address has no primary name', async () => {
    resolve.mockResolvedValue(null);
    const { container, queryClient } = renderWithProviders(
      <SoranPrimaryName address={ACCOUNT} />
    );

    await waitFor(() =>
      expect(
        queryClient.getQueryState(['soranPrimary', 'testnet', ACCOUNT])?.status
      ).toBe('success')
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('keeps lookup errors silent without retrying', async () => {
    resolve.mockRejectedValue(new Error('Soran unavailable'));
    const { container, queryClient } = renderWithProviders(
      <SoranPrimaryName address={ACCOUNT} />
    );

    await waitFor(() =>
      expect(
        queryClient.getQueryState(['soranPrimary', 'testnet', ACCOUNT])?.status
      ).toBe('error')
    );
    expect(container).toBeEmptyDOMElement();
    expect(resolve).toHaveBeenCalledTimes(1);
  });

  it('removes the previous name immediately when the address changes', async () => {
    let finishNext: (name: string) => void = () => undefined;
    const nextName = new Promise<string>((finish) => {
      finishNext = finish;
    });
    resolve.mockResolvedValueOnce('alice.nova').mockReturnValueOnce(nextName);
    const { rerender } = renderWithProviders(
      <SoranPrimaryName address={ACCOUNT} />
    );
    expect(await screen.findByText('alice.nova')).toBeInTheDocument();

    rerender(<SoranPrimaryName address={CONTRACT} />);

    expect(screen.queryByText('alice.nova')).not.toBeInTheDocument();
    await waitFor(() => expect(resolve).toHaveBeenCalledTimes(2));
    await act(async () => finishNext('pay.alice.nova'));
    expect(await screen.findByText('pay.alice.nova')).toBeInTheDocument();
  });
});
