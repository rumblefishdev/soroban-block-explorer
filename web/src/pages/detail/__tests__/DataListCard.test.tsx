import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';

import { DataListCard } from '../DataListCard.js';

type Row = { id: string };

const ROWS: Row[] = [{ id: 'row-a' }, { id: 'row-b' }];

function baseProps() {
  return {
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
    emptyNoun: 'transactions',
    canPrev: false,
    canNext: true,
    onPrev: vi.fn(),
    onNext: vi.fn(),
  };
}

// The state chain itself is covered by `DataList.test.tsx`; these cover what
// DataListCard adds — the Card — and that it forwards every prop.
describe('DataListCard', () => {
  it('wraps filters, body and pager in a Card', () => {
    const props = baseProps();
    const { container } = renderWithProviders(
      <DataListCard<Row>
        {...props}
        emptyKind="transactions"
        filters={<div>filters</div>}
      />
    );
    const cards = container.querySelectorAll('.MuiCard-root');
    expect(cards).toHaveLength(1);
    const [card] = cards;
    expect(card).toContainElement(screen.getByText('filters'));
    expect(card).toContainElement(screen.getByText('row-a'));
    expect(card).toContainElement(screen.getByText('Latest results'));
    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    expect(props.onNext).toHaveBeenCalledTimes(1);
  });

  it('forwards the empty and error props to DataList', () => {
    const { unmount } = renderWithProviders(
      <DataListCard<Row>
        {...baseProps()}
        rows={[]}
        renderEmpty={() => <div>custom empty</div>}
      />
    );
    expect(screen.getByText('custom empty')).toBeInTheDocument();
    unmount();

    const onRetry = vi.fn();
    renderWithProviders(
      <DataListCard<Row>
        {...baseProps()}
        emptyKind="transactions"
        isError
        error={new Error('boom')}
        onRetry={onRetry}
      />
    );
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('keeps the exclusive emptyKind / renderEmpty union', () => {
    const base = baseProps();
    // Never rendered: the `@ts-expect-error`s fail `typecheck` if the card's
    // props stop rejecting these calls.
    const invalid = [
      () => (
        // @ts-expect-error — `emptyKind` and `renderEmpty` are exclusive.
        <DataListCard<Row>
          {...base}
          emptyKind="ledgers"
          renderEmpty={() => null}
        />
      ),
      () => (
        // @ts-expect-error — one of `emptyKind` / `renderEmpty` is required.
        <DataListCard<Row> {...base} />
      ),
    ];
    expect(invalid).toHaveLength(2);
  });
});
