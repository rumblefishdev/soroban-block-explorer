import { screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';
import { NetworkSwitcher } from '../NetworkSwitcher.js';

describe('NetworkSwitcher', () => {
  it('links to both deployments', () => {
    renderWithProviders(<NetworkSwitcher current="mainnet" />);
    const nav = screen.getByRole('navigation', { name: 'Network' });
    expect(within(nav).getByRole('link', { name: 'Mainnet' })).toHaveAttribute(
      'href',
      'https://sorobanscan.rumblefish.dev'
    );
    expect(within(nav).getByRole('link', { name: 'Testnet' })).toHaveAttribute(
      'href',
      'https://testnet.sorobanscan.rumblefish.dev'
    );
  });

  it('marks testnet as current on a testnet build', () => {
    renderWithProviders(<NetworkSwitcher current="testnet" />);
    expect(screen.getByRole('link', { name: 'Testnet' })).toHaveAttribute(
      'aria-current',
      'page'
    );
    expect(screen.getByRole('link', { name: 'Mainnet' })).not.toHaveAttribute(
      'aria-current'
    );
  });

  it('marks mainnet as current on a mainnet build', () => {
    renderWithProviders(<NetworkSwitcher current="mainnet" />);
    expect(screen.getByRole('link', { name: 'Mainnet' })).toHaveAttribute(
      'aria-current',
      'page'
    );
    expect(screen.getByRole('link', { name: 'Testnet' })).not.toHaveAttribute(
      'aria-current'
    );
  });
});
