import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ExplorerThemeProvider } from '../../theme/ThemeProvider.js';

import { QueryErrorState } from './QueryErrorState.js';

function apiError(status: number, message: string) {
  return Object.assign(new Error(message), { status });
}

function renderState(error: unknown, onRetry = vi.fn()) {
  return render(
    <ExplorerThemeProvider>
      <QueryErrorState error={error} onRetry={onRetry} />
    </ExplorerThemeProvider>
  );
}

describe('QueryErrorState', () => {
  it('shows the API message and no retry for a rejected filter (400)', () => {
    renderState(
      apiError(400, 'asset pair must be `A/B`, each side at least 2 characters')
    );
    expect(screen.getByText(/That filter isn't valid/)).toBeTruthy();
    expect(screen.getByText(/each side at least 2 characters/)).toBeTruthy();
    // Resending identical input fails identically — offering a retry would
    // be a button that cannot work.
    expect(screen.queryByRole('button', { name: /try again/i })).toBeNull();
  });

  it('keeps the generic state and its retry for an unclassified failure', () => {
    renderState(new Error('boom'));
    expect(screen.getByText(/Something went wrong/)).toBeTruthy();
    expect(screen.getByRole('button', { name: /try again/i })).toBeTruthy();
  });

  it('keeps the transient state for a server error', () => {
    renderState(apiError(503, 'upstream unavailable'));
    expect(screen.queryByText(/That filter isn't valid/)).toBeNull();
    expect(screen.getByRole('button', { name: /try again/i })).toBeTruthy();
  });
});
