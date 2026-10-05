import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';
import { NetworkSwitcher, sectionOf } from '../NetworkSwitcher.js';

describe('NetworkSwitcher', () => {
  it('on mainnet, marks Mainnet current and links to testnet', () => {
    renderWithProviders(<NetworkSwitcher current="mainnet" />);
    expect(screen.getByText('Mainnet').closest('[aria-current]')).toHaveStyle({
      color: '#155dfc',
      backgroundColor: '#dbeafe',
    });
    expect(screen.queryByRole('link', { name: 'Mainnet' })).toBeNull();
    expect(screen.getByRole('link', { name: 'Testnet' })).toHaveAttribute(
      'href',
      'https://testnet.sorobanscan.rumblefish.dev'
    );
  });

  it('on testnet, marks Testnet current in amber and links to mainnet', () => {
    renderWithProviders(<NetworkSwitcher current="testnet" />);
    expect(screen.getByText('Testnet').closest('[aria-current]')).toHaveStyle({
      color: '#bb4d00',
      backgroundColor: '#fef3c6',
    });
    expect(screen.queryByRole('link', { name: 'Testnet' })).toBeNull();
    expect(screen.getByRole('link', { name: 'Mainnet' })).toHaveAttribute(
      'href',
      'https://sorobanscan.rumblefish.dev'
    );
  });

  it('lands a detail page on its section list of the other network', () => {
    renderWithProviders(<NetworkSwitcher current="mainnet" />, {
      initialEntries: ['/transactions/abc123'],
    });
    expect(screen.getByRole('link', { name: 'Testnet' })).toHaveAttribute(
      'href',
      'https://testnet.sorobanscan.rumblefish.dev/transactions'
    );
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
