import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ExplorerThemeProvider } from '../../theme/ThemeProvider.js';

import { SecondaryNav } from '../SecondaryNav.js';

describe('SecondaryNav', () => {
  // The portal is its own SPA: through `navItems` the click would reach the
  // router, which renders `/prices-api/` as a 404. Rendering with no `navItems`
  // proves the link lives outside them.
  it('links the Prices API portal with a plain anchor', () => {
    render(
      <ExplorerThemeProvider>
        <SecondaryNav
          logo={<span>logo</span>}
          navItems={[]}
          pricesApiUrl="/prices-api/"
        />
      </ExplorerThemeProvider>
    );

    expect(screen.getByRole('link', { name: 'Prices API' })).toHaveAttribute(
      'href',
      '/prices-api/'
    );
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
