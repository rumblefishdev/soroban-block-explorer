import { act, fireEvent, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';
import { NetworkSwitcher, sectionOf } from '../NetworkSwitcher.js';

describe('NetworkSwitcher', () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('on mainnet, names mainnet and links to testnet', () => {
    renderWithProviders(<NetworkSwitcher current="mainnet" />);
    expect(
      screen.getByRole('link', {
        name: 'Network: Mainnet. Switch to Testnet',
      })
    ).toHaveAttribute('href', 'https://testnet.sorobanscan.rumblefish.dev');
    expect(screen.getByText('Mainnet')).toHaveStyle({ color: '#155dfc' });
  });

  it('on testnet, highlights testnet in amber and links to mainnet', () => {
    renderWithProviders(<NetworkSwitcher current="testnet" />);
    expect(
      screen.getByRole('link', {
        name: 'Network: Testnet. Switch to Mainnet',
      })
    ).toHaveAttribute('href', 'https://sorobanscan.rumblefish.dev');
    expect(screen.getByText('Testnet')).toHaveStyle({ color: '#bb4d00' });
  });

  it('lands a detail page on its section list of the other network', () => {
    renderWithProviders(<NetworkSwitcher current="mainnet" />, {
      initialEntries: ['/transactions/abc123'],
    });
    expect(screen.getByRole('link')).toHaveAttribute(
      'href',
      'https://testnet.sorobanscan.rumblefish.dev/transactions'
    );
  });

  it('slides the highlight across, then opens the other network', () => {
    vi.useFakeTimers();
    const assign = vi.fn();
    vi.stubGlobal('location', { ...window.location, assign });
    renderWithProviders(<NetworkSwitcher current="mainnet" />);

    fireEvent.click(screen.getByRole('link'));
    expect(screen.getByText('Testnet')).toHaveStyle({ color: '#bb4d00' });
    expect(assign).not.toHaveBeenCalled();

    act(() => {
      vi.runAllTimers();
    });
    expect(assign).toHaveBeenCalledWith(
      'https://testnet.sorobanscan.rumblefish.dev'
    );
  });

  it('leaves a modified click to the browser', () => {
    vi.useFakeTimers();
    const assign = vi.fn();
    vi.stubGlobal('location', { ...window.location, assign });
    renderWithProviders(<NetworkSwitcher current="mainnet" />);

    fireEvent.click(screen.getByRole('link'), { ctrlKey: true });
    act(() => {
      vi.runAllTimers();
    });
    expect(assign).not.toHaveBeenCalled();
    expect(screen.getByText('Mainnet')).toHaveStyle({ color: '#155dfc' });
  });
});

describe('sectionOf', () => {
  it.each([
    ['/', ''],
    ['/transactions', '/transactions'],
    ['/transactions/', '/transactions'],
    ['/transactions/abc123', '/transactions'],
    ['/ledgers/64780770', '/ledgers'],
    ['/nfts/CABC/token-1', '/nfts'],
    ['/search', ''],
    ['/privacy-policy', ''],
    ['/transactionsx', ''],
    ['/no-such-page', ''],
  ])('%s → "%s"', (pathname, section) => {
    expect(sectionOf(pathname)).toBe(section);
  });
});
