import { screen, within } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';
import { NetworkPill } from '../NetworkPill.js';

describe('NetworkPill', () => {
  it('names testnet on a testnet build', () => {
    renderWithProviders(<NetworkPill current="testnet" />);
    expect(
      screen.getByRole('button', { name: 'Network: Testnet' })
    ).toHaveTextContent('Testnet');
  });

  it('names mainnet on a mainnet build', () => {
    renderWithProviders(<NetworkPill current="mainnet" />);
    expect(
      screen.getByRole('button', { name: 'Network: Mainnet' })
    ).toHaveTextContent('Mainnet');
  });

  it('opens a menu that links to both deployments and marks the current one', async () => {
    const user = userEvent.setup();
    renderWithProviders(<NetworkPill current="testnet" />);
    await user.click(screen.getByRole('button', { name: 'Network: Testnet' }));

    const menu = screen.getByRole('menu');
    const mainnet = within(menu).getByRole('menuitem', { name: /Mainnet/ });
    const testnet = within(menu).getByRole('menuitem', { name: /Testnet/ });
    expect(mainnet).toHaveAttribute(
      'href',
      'https://sorobanscan.rumblefish.dev'
    );
    expect(testnet).toHaveAttribute(
      'href',
      'https://testnet.sorobanscan.rumblefish.dev'
    );
    expect(testnet).toHaveAttribute('aria-current', 'page');
    expect(mainnet).not.toHaveAttribute('aria-current');
  });
});
