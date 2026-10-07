import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ExplorerThemeProvider } from '../../theme/ThemeProvider.js';

import { SecondaryNav } from '../SecondaryNav.js';

describe('SecondaryNav', () => {
  // The portal is its own SPA: through `navItems` the click would reach the
  // router, which renders `/prices-api/` as a 404. Rendering with no `navItems`
  // proves the link lives outside them. The app decides the URL per network
  // (testnet points at mainnet's portal), so both the inline link and the
  // drawer's must render the one they are given.
  it('links the Prices API portal with a plain anchor, inline and in the drawer', () => {
    render(
      <ExplorerThemeProvider>
        <SecondaryNav
          logo={<span>logo</span>}
          navItems={[]}
          pricesApiUrl="https://mainnet.example/prices-api/"
        />
      </ExplorerThemeProvider>
    );
    fireEvent.click(
      screen.getByRole('button', { name: 'Open navigation menu' })
    );

    // Read the DOM directly: the open drawer is modal and hides the inline
    // link from the accessibility tree, though it is still rendered.
    const links = [...document.querySelectorAll('a')].filter(
      (a) => a.textContent === 'Prices API'
    );
    expect(links).toHaveLength(2);
    for (const link of links) {
      expect(link).toHaveAttribute(
        'href',
        'https://mainnet.example/prices-api/'
      );
    }
  });

  it('puts the drawer footer at the bottom of the drawer', () => {
    render(
      <ExplorerThemeProvider>
        <SecondaryNav
          logo={<span>logo</span>}
          navItems={[]}
          pricesApiUrl="/prices-api/"
          drawerFooter={<span>network choice</span>}
        />
      </ExplorerThemeProvider>
    );

    expect(screen.queryByText('network choice')).toBeNull();
    fireEvent.click(
      screen.getByRole('button', { name: 'Open navigation menu' })
    );
    expect(screen.getByText('network choice')).toBeInTheDocument();
  });
});
