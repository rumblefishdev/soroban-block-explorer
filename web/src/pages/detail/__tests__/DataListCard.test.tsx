import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';

import { DataListCard } from '../DataListCard.js';

type Row = { id: string };

const ROWS: Row[] = [{ id: 'row-a' }, { id: 'row-b' }];

function baseProps(rows: Row[] = ROWS) {
  return {
    query: {
      isLoading: false,
      isPlaceholderData: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    },
    pager: {
      rows,
      canPrev: false,
      canNext: true,
      handlePrev: vi.fn(),
      handleNext: vi.fn(),
    },
    renderTable: (visible: readonly Row[]) => (
      <ul>
        {visible.map((row) => (
          <li key={row.id}>{row.id}</li>
        ))}
      </ul>
    ),
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
    expect(props.pager.handleNext).toHaveBeenCalledTimes(1);
  });

  it('forwards the empty and error props to DataList', () => {
    const { unmount } = renderWithProviders(
      <DataListCard<Row>
        {...baseProps([])}
        renderEmpty={() => <div>custom empty</div>}
      />
    );
    expect(screen.getByText('custom empty')).toBeInTheDocument();
    unmount();

    const props = baseProps();
    renderWithProviders(
      <DataListCard<Row>
        {...props}
        query={{ ...props.query, isError: true, error: new Error('boom') }}
        emptyKind="transactions"
      />
    );
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(props.query.refetch).toHaveBeenCalledTimes(1);
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
