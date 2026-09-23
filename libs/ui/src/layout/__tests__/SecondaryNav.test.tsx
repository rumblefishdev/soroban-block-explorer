import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ExplorerThemeProvider } from '../../theme/ThemeProvider.js';

import { SecondaryNav } from '../SecondaryNav.js';

describe('SecondaryNav', () => {
  // The portal is its own SPA: through `navItems` the click would reach the
  // router, which renders `/api/` as a 404. Rendering with no `navItems`
  // proves the link lives outside them.
  it('links the Prices API portal with a plain anchor', () => {
    render(
      <ExplorerThemeProvider>
        <SecondaryNav logo={<span>logo</span>} navItems={[]} />
      </ExplorerThemeProvider>
    );

    expect(screen.getByRole('link', { name: 'Prices API' })).toHaveAttribute(
      'href',
      '/api/'
    );
  });
});
