import { onlineManager } from '@tanstack/react-query';
import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { Route, Routes, useLocation } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import SearchResultsPage from '../pages/SearchResultsPage.js';
import { HomeHero } from '../pages/home/HomeHero.js';
import { emptySearchState, renderWithProviders } from '../test-utils.js';
import { GlobalSearchBar } from './GlobalSearchBar.js';
import {
  resolveSoranPayment,
  type SoranMemo,
  type SoranResolve,
} from './soran.js';
import {
  useSearchResults,
  type SearchResultsState,
} from './useSearchResults.js';

const current = vi.hoisted(() => ({
  network: 'testnet' as 'mainnet' | 'testnet',
}));

vi.mock('../network.js', () => ({
  get network() {
    return current.network;
  },
}));

vi.mock('./useSearchResults.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./useSearchResults.js')>();
  // React Query keeps unchanged indexed data stable across UI-only renders.
  const indexedStates = new Map<string, SearchResultsState>();
  return {
    ...actual,
    useSearchResults: vi.fn((params: { q: string }): SearchResultsState => {
      if (!['acme.token', 'mixed.nova'].includes(params.q)) {
        return emptySearchState(params.q);
      }
      const cached = indexedStates.get(params.q);
      if (cached) return cached;

      const hits: SearchResultsState['hitsForActiveTab'] = [
        {
          entity_type: 'account',
          identifier: params.q === 'mixed.nova' ? OTHER_ACCOUNT : ACCOUNT,
          label: 'Indexed dotted-name match',
        },
      ];
      const state = emptySearchState(params.q, {
        activeTab: 'account',
        counts: {
          transaction: 0,
          account: 1,
          contract: 0,
          asset: 0,
          nft: 0,
          pool: 0,
        },
        totalCount: 1,
        hitsForActiveTab: hits,
        data: { groups: { accounts: hits } } as SearchResultsState['data'],
      });
      indexedStates.set(params.q, state);
      return state;
    }),
  };
});

vi.mock('./soran.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./soran.js')>();
  return { ...actual, resolveSoranPayment: vi.fn() };
});

const ACCOUNT = 'GC526FUILJ6NLFXKCOOGTMDXNRW7MYSEK2UNRJV5FYWOGYDE4LOKXFEM';
const OTHER_ACCOUNT =
  'GA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVSGZ';
const CONTRACT = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM';
const MUXED =
  'MD5SL5RIC5STHGDDOJGSIIZHZZQPA4HIYFEPIQ3FF7M3H2F5VQ7K2AAAAAANF5TODJEYA';
const SEARCH_LABEL = 'Search by TX hash, accounts, contract, token';

function paymentResult(
  address = ACCOUNT,
  memo: SoranMemo = { type: 'none' }
): SoranResolve {
  return { kind: 'resolved', payment: { address, memo } };
}

function stubPayment(response = paymentResult()) {
  const resolveMock = vi
    .mocked(resolveSoranPayment)
    .mockResolvedValue(response);
  return resolveMock;
}

async function advanceLookup(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
  // React Query schedules its result notification after the render that
  // enabled the request, so flush that notification separately.
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
}

function bar(
  q: string,
  registerEnterHandler: (handler: () => boolean) => void = () => undefined,
  onDismiss: () => void = () => undefined,
  registerKeyHandler?: (handler: (key: string) => boolean) => void
) {
  return (
    <GlobalSearchBar
      q={q}
      onDismiss={onDismiss}
      registerEnterHandler={registerEnterHandler}
      registerKeyHandler={registerKeyHandler}
    />
  );
}

function renderSearch(q: string) {
  return renderWithProviders(
    <Routes>
      <Route path="/search" element={<SearchResultsPage />} />
      <Route path="/accounts/:accountId" element={<div>account page</div>} />
    </Routes>,
    { initialEntries: [`/search?q=${encodeURIComponent(q)}`] }
  );
}

function CurrentLocation() {
  const location = useLocation();
  return (
    <output aria-label="Current location">
      {location.pathname}
      {location.search}
    </output>
  );
}

beforeEach(() => {
  current.network = 'testnet';
  vi.mocked(useSearchResults).mockClear();
  vi.mocked(resolveSoranPayment).mockReset();
});

afterEach(() => {
  onlineManager.setOnline(true);
  vi.useRealTimers();
});

describe('Soran search on Testnet', () => {
  it('waits 350 ms after the latest edit before resolving the complete name', async () => {
    vi.useFakeTimers();
    const resolveMock = stubPayment();
    const { rerender } = renderWithProviders(bar('alice.nov'));

    await advanceLookup(250);
    expect(resolveMock).not.toHaveBeenCalled();
    rerender(bar('alice.nova'));
    await advanceLookup(349);

    expect(screen.getByText('Searching…')).toBeInTheDocument();
    expect(resolveMock).not.toHaveBeenCalled();
    await advanceLookup(1);

    expect(resolveMock).toHaveBeenCalledTimes(1);
    expect(resolveMock).toHaveBeenCalledWith(
      'alice.nova',
      'testnet',
      expect.any(AbortSignal)
    );
    expect(screen.getByRole('link', { name: ACCOUNT })).toBeInTheDocument();
  });

  it('automatically puts an account destination in the native Accounts tab', async () => {
    const resolveMock = stubPayment();
    renderWithProviders(bar('alice.nova'));

    expect(screen.getByText('Searching…')).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /Soran/ })
    ).not.toBeInTheDocument();

    expect(await screen.findByRole('link', { name: ACCOUNT })).toHaveAttribute(
      'href',
      `/accounts/${ACCOUNT}?soran=alice.nova`
    );
    expect(screen.getByRole('tab', { name: 'Accounts 1' })).toHaveAttribute(
      'aria-selected',
      'true'
    );
    expect(screen.getByRole('tab', { name: 'Contract 0' })).toBeInTheDocument();
    expect(screen.getByText('alice.nova')).toBeInTheDocument();
    expect(screen.queryByText(/· Soran/)).not.toBeInTheDocument();
    expect(screen.queryByText(/No results for/)).not.toBeInTheDocument();
    expect(resolveMock).toHaveBeenCalledTimes(1);
    expect(resolveMock).toHaveBeenCalledWith(
      'alice.nova',
      'testnet',
      expect.any(AbortSignal)
    );
  });

  it('skips the debounce through Enter without repeating the automatic lookup', async () => {
    vi.useFakeTimers();
    const resolveMock = stubPayment();
    let enter: () => boolean = () => false;
    renderWithProviders(
      bar('alice.nova', (handler) => {
        enter = handler;
      })
    );

    expect(resolveMock).not.toHaveBeenCalled();
    await act(async () => {
      expect(enter()).toBe(true);
    });
    await advanceLookup(0);

    expect(screen.getByRole('link', { name: ACCOUNT })).toBeInTheDocument();
    expect(resolveMock).toHaveBeenCalledTimes(1);
    await advanceLookup(350);
    expect(resolveMock).toHaveBeenCalledTimes(1);
  });

  it('resolves a search deep link and keeps the required memo beside the account', async () => {
    const resolveMock = stubPayment(
      paymentResult(ACCOUNT, { type: 'id', value: '18446744073709551615' })
    );
    renderSearch('alice.nova');

    expect(await screen.findByRole('link', { name: ACCOUNT })).toHaveAttribute(
      'href',
      `/accounts/${ACCOUNT}?soran=alice.nova`
    );
    expect(
      screen.getByText('Required memo (id): 18446744073709551615')
    ).toBeInTheDocument();
    expect(screen.queryByText('account page')).not.toBeInTheDocument();
    expect(resolveMock).toHaveBeenCalledTimes(1);
  });

  it('links a contract payment destination to its contract page', async () => {
    stubPayment(paymentResult(CONTRACT));
    renderSearch('alice.nova');

    expect(await screen.findByRole('link', { name: CONTRACT })).toHaveAttribute(
      'href',
      `/contracts/${CONTRACT}`
    );
    expect(screen.getByRole('tab', { name: 'Contract 1' })).toHaveAttribute(
      'aria-selected',
      'true'
    );
    expect(screen.getByRole('tab', { name: 'Accounts 0' })).toBeInTheDocument();
  });

  it('puts a muxed destination in Accounts and copies it without a detail link', async () => {
    const user = userEvent.setup();
    stubPayment(paymentResult(MUXED));
    renderSearch('alice.nova');

    expect(await screen.findByText(MUXED)).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Accounts 1' })).toHaveAttribute(
      'aria-selected',
      'true'
    );
    expect(screen.queryByRole('link', { name: MUXED })).not.toBeInTheDocument();
    await user.click(
      screen.getByRole('button', {
        name: /Copy.*(?:address|destination|clipboard)/i,
      })
    );
    expect(await navigator.clipboard.readText()).toBe(MUXED);
  });

  it('preserves whitespace in the required text memo and copies the exact value', async () => {
    const user = userEvent.setup();
    const memo = '  alice  nova  ';
    const onDismiss = vi.fn();
    stubPayment(paymentResult(ACCOUNT, { type: 'text', value: memo }));
    renderWithProviders(
      <>
        {bar('alice.nova', undefined, onDismiss)}
        <CurrentLocation />
      </>
    );

    await screen.findByRole('link', { name: ACCOUNT });
    const memoText = screen.getByText(`Required memo (text): ${memo}`, {
      normalizer: (value) => value,
    });
    expect(memoText).toHaveStyle({ whiteSpace: 'pre-wrap' });
    await user.click(
      screen.getByRole('button', { name: 'Copy required Soran memo' })
    );

    expect(await navigator.clipboard.readText()).toBe(memo);
    expect(onDismiss).not.toHaveBeenCalled();
    expect(screen.getByLabelText('Current location').textContent).toBe('/');
  });

  it('dismisses the dropdown when a native Soran account row is clicked', async () => {
    const user = userEvent.setup();
    const onDismiss = vi.fn();
    stubPayment(paymentResult(ACCOUNT, { type: 'id', value: '77' }));
    renderWithProviders(
      <>
        {bar('alice.nova', undefined, onDismiss)}
        <CurrentLocation />
      </>
    );

    await user.click(await screen.findByRole('link', { name: ACCOUNT }));

    expect(onDismiss).toHaveBeenCalledTimes(1);
    expect(screen.getByLabelText('Current location')).toHaveTextContent(
      `/accounts/${ACCOUNT}?soran=alice.nova`
    );
  });

  it('uses ArrowDown and Enter to select a native Soran row with its memo URL', async () => {
    const onDismiss = vi.fn();
    let enter: () => boolean = () => false;
    const resolveMock = stubPayment(
      paymentResult(ACCOUNT, { type: 'id', value: '77' })
    );
    renderWithProviders(
      <>
        {bar(
          'alice.nova',
          (handler) => {
            enter = handler;
          },
          onDismiss
        )}
        <CurrentLocation />
      </>
    );
    await screen.findByRole('link', { name: ACCOUNT });

    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'ArrowDown' });
    await act(async () => {
      expect(enter()).toBe(true);
    });

    expect(onDismiss).toHaveBeenCalledTimes(1);
    expect(screen.getByLabelText('Current location')).toHaveTextContent(
      `/accounts/${ACCOUNT}?soran=alice.nova`
    );
    expect(resolveMock).toHaveBeenCalledTimes(1);
  });

  it('uses the registered ArrowUp handler to select the last of two account rows initially', async () => {
    const onDismiss = vi.fn();
    let enter: () => boolean = () => false;
    let keyDown: (key: string) => boolean = () => false;
    const resolveMock = stubPayment();
    renderWithProviders(
      <>
        {bar(
          'mixed.nova',
          (handler) => {
            enter = handler;
          },
          onDismiss,
          (handler) => {
            keyDown = handler;
          }
        )}
        <CurrentLocation />
      </>
    );
    await screen.findByRole('link', { name: ACCOUNT });
    const rows = screen.getAllByRole('link');
    expect(rows).toHaveLength(2);
    expect(rows[1]).toHaveAttribute('href', `/accounts/${OTHER_ACCOUNT}`);

    act(() => {
      expect(keyDown('ArrowUp')).toBe(true);
    });
    await act(async () => {
      expect(enter()).toBe(true);
    });

    expect(screen.getByLabelText('Current location').textContent).toBe(
      `/accounts/${OTHER_ACCOUNT}`
    );
    expect(onDismiss).toHaveBeenCalledTimes(1);
    expect(resolveMock).toHaveBeenCalledTimes(1);
  });

  it.each(['header', 'results page'])(
    'uses native empty results for a missing name in the %s',
    async (surface) => {
      stubPayment({
        kind: 'failed',
        code: 'not_found',
        reason: 'Soran namespace was not found on Testnet.',
      });
      if (surface === 'header') renderWithProviders(bar('robert.no'));
      else renderSearch('robert.no');

      expect(
        await screen.findByText('No results for "robert.no"')
      ).toBeInTheDocument();
      expect(
        screen.queryByText(/namespace was not found/)
      ).not.toBeInTheDocument();
      expect(
        screen.queryByRole('button', { name: 'Try again' })
      ).not.toBeInTheDocument();
      expect(screen.queryByText('Searching…')).not.toBeInTheDocument();
    }
  );

  it('keeps indexed matches when a dotted query has no Soran name', async () => {
    const resolveMock = stubPayment({
      kind: 'failed',
      code: 'not_found',
      reason: 'Soran namespace was not found on Testnet.',
    });
    renderSearch('mixed.nova');
    await waitFor(() => expect(resolveMock).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(screen.queryByText('Searching…')).not.toBeInTheDocument()
    );
    expect(
      screen.getByRole('link', { name: /Indexed dotted-name match/ })
    ).toBeInTheDocument();
    expect(screen.queryByText(/No results for/)).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Try again' })
    ).not.toBeInTheDocument();
  });

  it('lets a failed lookup be retried explicitly', async () => {
    const user = userEvent.setup();
    const resolveMock = stubPayment();
    resolveMock.mockReset();
    resolveMock
      .mockResolvedValueOnce({
        kind: 'failed',
        reason: 'Soran is temporarily unavailable.',
      })
      .mockResolvedValueOnce(paymentResult());
    renderSearch('alice.nova');

    const retry = await screen.findByRole('button', {
      name: 'Try again',
    });
    expect(resolveMock).toHaveBeenCalledTimes(1);
    expect(screen.queryByText(/No results for/)).not.toBeInTheDocument();
    expect(
      screen.queryByRole('link', { name: ACCOUNT })
    ).not.toBeInTheDocument();
    await user.click(retry);

    expect(
      await screen.findByRole('link', { name: ACCOUNT })
    ).toBeInTheDocument();
    expect(resolveMock).toHaveBeenCalledTimes(2);
  });

  it('hides the prior row and memo when a fresh lookup is paused offline', async () => {
    const user = userEvent.setup();
    const resolveMock = stubPayment(
      paymentResult(ACCOUNT, { type: 'id', value: '77' })
    );
    const { queryClient } = renderSearch('alice.nova');
    await screen.findByRole('link', { name: ACCOUNT });
    expect(screen.getByText('Required memo (id): 77')).toBeInTheDocument();

    act(() => onlineManager.setOnline(false));
    await user.type(screen.getByLabelText(SEARCH_LABEL), '{Enter}');
    await waitFor(() => {
      expect(
        queryClient.getQueryState(['soranPayment', 'testnet', 'alice.nova'])
          ?.fetchStatus
      ).toBe('paused');
    });

    expect(
      await screen.findByText('Offline. Reconnect to look up this Soran name.')
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('link', { name: ACCOUNT })
    ).not.toBeInTheDocument();
    expect(
      screen.queryByText('Required memo (id): 77')
    ).not.toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Accounts 0' })).toBeInTheDocument();
    expect(resolveMock).toHaveBeenCalledTimes(1);

    resolveMock.mockResolvedValueOnce(
      paymentResult(ACCOUNT, { type: 'id', value: '88' })
    );
    act(() => onlineManager.setOnline(true));
    expect(
      await screen.findByText('Required memo (id): 88')
    ).toBeInTheDocument();
    expect(
      screen.queryByText('Required memo (id): 77')
    ).not.toBeInTheDocument();
    expect(screen.getByRole('link', { name: ACCOUNT })).toBeInTheDocument();
    expect(resolveMock).toHaveBeenCalledTimes(2);
  });

  it('removes the previous result on edit and automatically resolves the new name', async () => {
    vi.useFakeTimers();
    const resolveMock = stubPayment();
    renderSearch('alice.nova');
    await advanceLookup(0);
    expect(screen.getByRole('link', { name: ACCOUNT })).toBeInTheDocument();

    const input = screen.getByLabelText(SEARCH_LABEL);
    fireEvent.change(input, { target: { value: 'bob.nova' } });

    expect(
      screen.queryByRole('link', { name: ACCOUNT })
    ).not.toBeInTheDocument();
    expect(screen.queryByText('alice.nova')).not.toBeInTheDocument();
    expect(screen.getByText('Searching…')).toBeInTheDocument();
    expect(resolveMock).toHaveBeenCalledTimes(1);

    resolveMock.mockResolvedValueOnce(paymentResult(CONTRACT));
    await advanceLookup(349);
    expect(resolveMock).toHaveBeenCalledTimes(1);
    await advanceLookup(1);

    expect(screen.getByRole('link', { name: CONTRACT })).toBeInTheDocument();
    expect(resolveMock).toHaveBeenCalledTimes(2);
    expect(resolveMock).toHaveBeenLastCalledWith(
      'bob.nova',
      'testnet',
      expect.any(AbortSignal)
    );
  });

  it('does not display an in-flight answer after the query has changed', async () => {
    vi.useFakeTimers();
    let finish: (value: SoranResolve) => void = () => undefined;
    const pending = new Promise<SoranResolve>((resolve) => {
      finish = resolve;
    });
    const resolveMock = vi.mocked(resolveSoranPayment).mockReturnValue(pending);
    renderSearch('alice.nova');
    expect(resolveMock).toHaveBeenCalledTimes(1);

    const input = screen.getByLabelText(SEARCH_LABEL);
    fireEvent.change(input, { target: { value: 'bob.nova' } });
    await act(async () => finish(paymentResult()));

    expect(
      screen.queryByRole('link', { name: ACCOUNT })
    ).not.toBeInTheDocument();
    expect(screen.getByText('Searching…')).toBeInTheDocument();
    expect(resolveMock).toHaveBeenCalledTimes(1);

    resolveMock.mockResolvedValueOnce(paymentResult(CONTRACT));
    await advanceLookup(350);
    expect(screen.getByRole('link', { name: CONTRACT })).toBeInTheDocument();
    expect(
      screen.queryByRole('link', { name: ACCOUNT })
    ).not.toBeInTheDocument();
    expect(resolveMock).toHaveBeenCalledTimes(2);
  });

  it('keeps distinct indexed matches and adds the resolved name to native counts', async () => {
    const resolveMock = stubPayment();
    renderWithProviders(bar('mixed.nova'));

    expect(screen.getByText('Indexed dotted-name match')).toBeInTheDocument();
    expect(useSearchResults).toHaveBeenCalledWith({ q: 'mixed.nova' });
    expect(resolveMock).not.toHaveBeenCalled();
    expect(
      await screen.findByRole('link', { name: ACCOUNT })
    ).toBeInTheDocument();
    expect(screen.getByText('Indexed dotted-name match')).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Accounts 2' })).toHaveAttribute(
      'aria-selected',
      'true'
    );
    expect(screen.getAllByRole('link')).toHaveLength(2);
    expect(screen.getByLabelText(OTHER_ACCOUNT)).toBeInTheDocument();
    expect(screen.getByText('mixed.nova')).toBeInTheDocument();
  });

  it('deduplicates the same indexed address while retaining the Soran name and memo', async () => {
    stubPayment(paymentResult(ACCOUNT, { type: 'id', value: '77' }));
    renderWithProviders(bar('acme.token'));
    expect(screen.getByText('Indexed dotted-name match')).toBeInTheDocument();

    expect(
      await screen.findByText('Required memo (id): 77')
    ).toBeInTheDocument();

    expect(screen.getByRole('tab', { name: 'Accounts 1' })).toHaveAttribute(
      'aria-selected',
      'true'
    );
    expect(screen.getAllByRole('link')).toHaveLength(1);
    expect(screen.getByRole('link', { name: ACCOUNT })).toHaveAttribute(
      'href',
      `/accounts/${ACCOUNT}?soran=acme.token`
    );
    expect(screen.getByText('acme.token')).toBeInTheDocument();
  });

  it.each(['dropdown', 'results page'])(
    'makes no Soran offer or request on mainnet in the %s',
    async (surface) => {
      vi.useFakeTimers();
      current.network = 'mainnet';
      const resolveMock = stubPayment();
      let enter: () => boolean = () => false;
      if (surface === 'dropdown') {
        renderWithProviders(
          bar('alice.nova', (handler) => {
            enter = handler;
          })
        );
        expect(enter()).toBe(false);
      } else {
        renderSearch('alice.nova');
      }
      await advanceLookup(700);

      expect(screen.queryByText('alice.nova')).not.toBeInTheDocument();
      expect(
        screen.queryByRole('button', { name: /Soran/ })
      ).not.toBeInTheDocument();
      expect(resolveMock).not.toHaveBeenCalled();
      expect(useSearchResults).toHaveBeenCalledWith({ q: 'alice.nova' });
    }
  );

  it('cancels the pending lookup when the name becomes incomplete or invalid', async () => {
    vi.useFakeTimers();
    const resolveMock = stubPayment();
    const { rerender } = renderWithProviders(bar('alice.nova'));
    await advanceLookup(250);

    for (const q of ['alice.', 'alice', 'bad!name.nova', '']) {
      rerender(bar(q));
      await advanceLookup(350);
      expect(screen.queryByText('Searching…')).not.toBeInTheDocument();
      expect(resolveMock).not.toHaveBeenCalled();
    }
  });

  it('hides a previous answer when editing away and returning to that name', async () => {
    vi.useFakeTimers();
    const resolveMock = stubPayment();
    const { rerender } = renderWithProviders(bar('alice.nova'));
    await advanceLookup(350);
    expect(screen.getByRole('link', { name: ACCOUNT })).toBeInTheDocument();

    rerender(bar('bob.nova'));
    rerender(bar('alice.nova'));

    expect(
      screen.queryByRole('link', { name: ACCOUNT })
    ).not.toBeInTheDocument();
    await advanceLookup(349);
    expect(resolveMock).toHaveBeenCalledTimes(1);
    await advanceLookup(1);
    expect(resolveMock).toHaveBeenCalledTimes(2);
    expect(screen.getByRole('link', { name: ACCOUNT })).toBeInTheDocument();
  });

  it('cancels scheduled lookups and hides results when switching to mainnet', async () => {
    vi.useFakeTimers();
    const resolveMock = stubPayment();
    const { rerender } = renderWithProviders(bar('alice.nova'));
    await advanceLookup(250);

    current.network = 'mainnet';
    rerender(bar('alice.nova'));
    await advanceLookup(700);
    expect(resolveMock).not.toHaveBeenCalled();
    expect(screen.queryByText('Searching…')).not.toBeInTheDocument();

    current.network = 'testnet';
    rerender(bar('alice.nova'));
    await advanceLookup(350);
    expect(screen.getByRole('link', { name: ACCOUNT })).toBeInTheDocument();

    current.network = 'mainnet';
    rerender(bar('alice.nova'));
    await advanceLookup(700);
    expect(
      screen.queryByRole('link', { name: ACCOUNT })
    ).not.toBeInTheDocument();
    expect(resolveMock).toHaveBeenCalledTimes(1);
  });

  it.each(['testnet', 'mainnet'] as const)(
    'automatically checks names typed in the home hero only on testnet (%s)',
    async (selectedNetwork) => {
      vi.useFakeTimers();
      current.network = selectedNetwork;
      const resolveMock = stubPayment();
      renderWithProviders(<HomeHero />);

      fireEvent.change(screen.getByLabelText(SEARCH_LABEL), {
        target: { value: 'alice.nova' },
      });
      expect(resolveMock).not.toHaveBeenCalled();
      await advanceLookup(350);

      if (selectedNetwork === 'testnet') {
        expect(screen.getByRole('link', { name: ACCOUNT })).toHaveAttribute(
          'href',
          `/accounts/${ACCOUNT}?soran=alice.nova`
        );
        expect(screen.getByRole('tab', { name: 'Accounts 1' })).toHaveAttribute(
          'aria-selected',
          'true'
        );
        expect(screen.getByText('alice.nova')).toBeInTheDocument();
        expect(resolveMock).toHaveBeenCalledTimes(1);
      } else {
        expect(screen.queryByText('Searching…')).not.toBeInTheDocument();
        expect(resolveMock).not.toHaveBeenCalled();
      }
      expect(
        screen.queryByRole('button', { name: /(?:Search|Look up).*Soran/i })
      ).not.toBeInTheDocument();
    }
  );

  it('preserves ordinary home hero search submission', async () => {
    const user = userEvent.setup();
    const resolveMock = stubPayment();
    renderWithProviders(
      <Routes>
        <Route path="/" element={<HomeHero />} />
        <Route path="/search" element={<SearchResultsPage />} />
      </Routes>
    );

    await user.type(screen.getByLabelText(SEARCH_LABEL), 'kale{Enter}');

    expect(screen.getByRole('heading', { name: 'Search' })).toBeInTheDocument();
    expect(screen.getByLabelText(SEARCH_LABEL)).toHaveValue('kale');
    expect(resolveMock).not.toHaveBeenCalled();
  });
});
