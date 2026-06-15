import { ThemeProvider } from '@mui/material/styles';
import { render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { createExplorerTheme } from '../theme/theme.js';

import { PaginationControls } from './PaginationControls.js';

const THEME = createExplorerTheme('dark');

function withTheme(ui: ReactNode) {
  return <ThemeProvider theme={THEME}>{ui}</ThemeProvider>;
}

describe('PaginationControls', () => {
  it('disables Previous when canPrev is false', () => {
    render(
      withTheme(<PaginationControls canPrev={false} canNext onPrev={vi.fn()} />)
    );
    expect(screen.getByRole('button', { name: 'Previous' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Next' })).toBeEnabled();
  });

  it('disables Next when canNext is false', () => {
    render(
      withTheme(<PaginationControls canPrev canNext={false} onNext={vi.fn()} />)
    );
    expect(screen.getByRole('button', { name: 'Next' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Previous' })).toBeEnabled();
  });

  it('fires onPrev / onNext on click', async () => {
    const user = userEvent.setup();
    const onPrev = vi.fn();
    const onNext = vi.fn();
    render(
      withTheme(
        <PaginationControls canPrev canNext onPrev={onPrev} onNext={onNext} />
      )
    );
    await user.click(screen.getByRole('button', { name: 'Previous' }));
    expect(onPrev).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole('button', { name: 'Next' }));
    expect(onNext).toHaveBeenCalledTimes(1);
  });

  it('renders the caption when provided', () => {
    render(
      withTheme(
        <PaginationControls
          canPrev={false}
          canNext={false}
          caption="Showing 1-20"
        />
      )
    );
    expect(screen.getByText('Showing 1-20')).toBeInTheDocument();
  });
});
