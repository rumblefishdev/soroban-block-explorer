import { Box } from '@mui/material';
import { fireEvent, screen } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';

import { DataListCard } from '../DataListCard.js';

type Row = { id: string };

type Props = ComponentProps<typeof DataListCard<Row>>;

const ROWS: Row[] = [{ id: 'row-a' }, { id: 'row-b' }];

function renderCard(overrides: Partial<Props> = {}) {
  const props = {
    columnCount: 3,
    isLoading: false,
    isError: false,
    rows: ROWS,
    renderTable: (rows: readonly Row[]) => (
      <ul>
        {rows.map((row) => (
          <li key={row.id}>{row.id}</li>
        ))}
      </ul>
    ),
    emptyKind: 'transactions',
    emptyNoun: 'transactions',
    canPrev: false,
    canNext: true,
    onPrev: vi.fn(),
    onNext: vi.fn(),
    ...overrides,
  } as Props;
  return { props, ...renderWithProviders(<DataListCard<Row> {...props} />) };
}

/** `EmptyState` padding box: title → text Stack → content Stack → `py` Box. */
function errorStatePaddingTop(): string {
  const box = screen.getByText('Something went wrong').closest('.MuiStack-root')
    ?.parentElement?.parentElement;
  if (!box) throw new Error('error-state padding box not found');
  return getComputedStyle(box).paddingTop;
}

describe('DataListCard', () => {
  it('renders the table and the pager when filled', () => {
    const { props } = renderCard();
    expect(screen.getByText('row-a')).toBeInTheDocument();
    expect(screen.getByText('Latest results')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Previous' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    expect(props.onNext).toHaveBeenCalledTimes(1);
  });

  it('shows the skeleton instead of the rows while loading or reloading', () => {
    const renderSkeleton = () => <div>skeleton</div>;
    const { unmount } = renderCard({ isLoading: true, renderSkeleton });
    expect(screen.getByText('skeleton')).toBeInTheDocument();
    expect(screen.queryByText('row-a')).not.toBeInTheDocument();
    unmount();

    // Reloading keeps the previous rows around; they must not show.
    renderCard({ isReloading: true, renderSkeleton });
    expect(screen.getByText('skeleton')).toBeInTheDocument();
    expect(screen.queryByText('row-a')).not.toBeInTheDocument();
  });

  it('falls back to the generic skeleton without renderSkeleton', () => {
    renderCard({ isLoading: true });
    expect(screen.queryByText('row-a')).not.toBeInTheDocument();
    // The pager stays mounted under the skeleton.
    expect(screen.getByText('Latest results')).toBeInTheDocument();
  });

  it('renders the error state with a retry, at py 8 unless overridden', () => {
    const onRetry = vi.fn();
    const { unmount } = renderCard({
      isError: true,
      error: new Error('boom'),
      onRetry,
    });
    expect(screen.getByText('Something went wrong')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(onRetry).toHaveBeenCalledTimes(1);
    const py8 = errorStatePaddingTop();
    unmount();

    renderCard({ isError: true, error: new Error('boom'), errorPy: 6 });
    const py6 = errorStatePaddingTop();
    expect(py8).toBe('64px');
    expect(py6).toBe('48px');
  });

  it('renders the emptyKind preset when there are no rows', () => {
    renderCard({ rows: [] });
    expect(screen.getByText('No transactions yet')).toBeInTheDocument();
  });

  it('renders renderEmpty instead of the preset', () => {
    renderCard({
      rows: [],
      emptyKind: undefined,
      renderEmpty: () => <div>custom empty</div>,
    });
    expect(screen.getByText('custom empty')).toBeInTheDocument();
    expect(screen.queryByText('No transactions yet')).not.toBeInTheDocument();
  });

  it('prefers the filtered-empty state over renderEmpty when filters are active', () => {
    const onClearFilters = vi.fn();
    renderCard({
      rows: [],
      emptyKind: undefined,
      renderEmpty: () => <div>custom empty</div>,
      hasActiveFilters: true,
      onClearFilters,
    });
    expect(
      screen.getByText('No transactions match your filters')
    ).toBeInTheDocument();
    expect(screen.queryByText('custom empty')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Clear filters' }));
    expect(onClearFilters).toHaveBeenCalledTimes(1);
  });

  it('wraps filters, body and pager in a Card by default', () => {
    const { container } = renderCard({ filters: <div>filters</div> });
    const card = container.querySelector('.MuiCard-root');
    expect(card).not.toBeNull();
    expect(card).toContainElement(screen.getByText('filters'));
    expect(card).toContainElement(screen.getByText('row-a'));
    expect(card).toContainElement(screen.getByText('Latest results'));
  });

  it('wraps the content in renderContainer instead of the Card', () => {
    const { container } = renderCard({
      renderContainer: (content) => (
        <Box data-testid="shell">
          <h2>Section title</h2>
          {content}
        </Box>
      ),
    });
    expect(container.querySelector('.MuiCard-root')).toBeNull();
    const shell = screen.getByTestId('shell');
    expect(shell).toContainElement(screen.getByText('Section title'));
    expect(shell).toContainElement(screen.getByText('row-a'));
    expect(shell).toContainElement(screen.getByText('Latest results'));
  });
});
