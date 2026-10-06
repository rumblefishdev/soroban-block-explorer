import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';
import { NetworkMenu } from '../NetworkMenu.js';

describe('NetworkMenu', () => {
  it('on testnet, ticks testnet and links to mainnet', () => {
    renderWithProviders(<NetworkMenu current="testnet" />);

    expect(screen.getByText('Testnet').closest('[aria-current]')).toBeTruthy();
    expect(screen.queryByRole('link', { name: 'Testnet' })).toBeNull();
    expect(screen.getByRole('link', { name: 'Mainnet' })).toHaveAttribute(
      'href',
      'https://sorobanscan.rumblefish.dev'
    );
  });

  it('lands a detail page on its section list of the other network', () => {
    renderWithProviders(<NetworkMenu current="mainnet" />, {
      initialEntries: ['/ledgers/123'],
    });

    expect(screen.getByRole('link', { name: 'Testnet' })).toHaveAttribute(
      'href',
      'https://testnet.sorobanscan.rumblefish.dev/ledgers'
    );
  });
});
