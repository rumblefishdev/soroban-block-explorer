import { ThemeProvider } from '@mui/material/styles';
import { render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import type { ComponentProps, ReactNode } from 'react';
import { afterEach, describe, expect, it } from 'vitest';

import { LinkComponentProvider } from '../../identifiers/LinkComponentContext.js';
import { createExplorerTheme } from '../../theme/theme.js';

import { Footer } from '../Footer.js';

const THEME = createExplorerTheme('dark');

function withTheme(ui: ReactNode) {
  return <ThemeProvider theme={THEME}>{ui}</ThemeProvider>;
}

afterEach(() => {
  delete window._hsp;
});

describe('Footer', () => {
  // The consent banner is the only way to withdraw consent once given, so the
  // control that re-opens it has to keep working through refactors of
  // `FooterLink` — hence the assertion on the queued command, not just on the
  // label being present.
  it('queues showBanner for HubSpot when Cookie Settings is clicked', async () => {
    const user = userEvent.setup();
    render(withTheme(<Footer logo={<span>logo</span>} navItems={[]} />));

    await user.click(screen.getByRole('link', { name: 'Cookie Settings' }));

    expect(window._hsp).toEqual([['showBanner']]);
  });

  it('links the Prices API portal under /api/', () => {
    render(withTheme(<Footer logo={<span>logo</span>} navItems={[]} />));

    expect(screen.getByRole('link', { name: 'Prices API' })).toHaveAttribute(
      'href',
      '/api/'
    );
  });

  // The policy is a route of this app: it goes through the router's link
  // (same tab, no reload), not out to a new tab like the external resources.
  it('links the privacy policy through the router, in the same tab', () => {
    const RouterLink = (props: ComponentProps<'a'>) => (
      <a data-router-link="" {...props} />
    );
    render(
      withTheme(
        <LinkComponentProvider value={RouterLink}>
          <Footer logo={<span>logo</span>} navItems={[]} />
        </LinkComponentProvider>
      )
    );

    const link = screen.getByRole('link', { name: 'Privacy Policy' });
    expect(link).toHaveAttribute('href', '/privacy-policy');
    expect(link).toHaveAttribute('data-router-link');
    expect(link).not.toHaveAttribute('target');
  });
});
