import { render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';

import { ExplorerThemeProvider, useColorMode } from './ThemeProvider.js';

const STORAGE_KEY = 'soroban-explorer.color-mode';

function Probe() {
  const { mode, toggleMode } = useColorMode();
  return (
    <button type="button" onClick={toggleMode}>
      {mode}
    </button>
  );
}

beforeEach(() => {
  window.localStorage.clear();
});

describe('ExplorerThemeProvider', () => {
  it('defaults to dark without writing that default to storage', () => {
    render(
      <ExplorerThemeProvider>
        <Probe />
      </ExplorerThemeProvider>
    );

    expect(screen.getByRole('button')).toHaveTextContent('dark');
    // A mount-time write would look like an explicit user choice on the next
    // visit and pin the visitor to today's default.
    expect(window.localStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  it('persists the mode once the user toggles it', async () => {
    render(
      <ExplorerThemeProvider>
        <Probe />
      </ExplorerThemeProvider>
    );

    await userEvent.click(screen.getByRole('button'));

    expect(screen.getByRole('button')).toHaveTextContent('light');
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe('light');
  });

  it('restores a stored choice over the default', () => {
    window.localStorage.setItem(STORAGE_KEY, 'light');

    render(
      <ExplorerThemeProvider>
        <Probe />
      </ExplorerThemeProvider>
    );

    expect(screen.getByRole('button')).toHaveTextContent('light');
  });
});
