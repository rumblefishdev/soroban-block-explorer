import {
  QueryErrorState,
  TableEmptyState,
  type TableEmptyKind,
} from '@rumblefish/soroban-block-explorer-ui';
import { fireEvent, screen } from '@testing-library/react';
import type { ReactElement, ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';

import {
  DataList,
  type DataListPager,
  type DataListProps,
  type DataListQuery,
} from '../DataList.js';

type Row = { id: string };

type BaseProps = Omit<
  DataListProps<Row>,
  | 'emptyKind'
  | 'emptyPy'
  | 'renderEmpty'
  | 'hasActiveFilters'
  | 'emptyNoun'
  | 'onClearFilters'
>;

type EmptyChoice =
  | { emptyKind: TableEmptyKind; emptyPy?: number }
  | { renderEmpty: () => ReactNode };

const ROWS: Row[] = [{ id: 'row-a' }, { id: 'row-b' }];

function query(overrides: Partial<DataListQuery> = {}): DataListQuery {
  return {
    isLoading: false,
    isPlaceholderData: false,
    isError: false,
    error: null,
    refetch: vi.fn(),
    ...overrides,
  };
}

function pager(overrides: Partial<DataListPager<Row>> = {}) {
  return {
    rows: ROWS,
    canPrev: false,
    canNext: true,
    handlePrev: vi.fn(),
    handleNext: vi.fn(),
    ...overrides,
  };
}

/** A table that says whether it was asked for its loading mode. */
function renderTable(rows: readonly Row[], { loading }: { loading: boolean }) {
  return loading ? (
    <div>skeleton ({rows.length} rows)</div>
  ) : (
    <ul>
      {rows.map((row) => (
        <li key={row.id}>{row.id}</li>
      ))}
    </ul>
  );
}

function baseProps(): BaseProps {
  return { query: query(), pager: pager(), renderTable };
}

function renderList(
  overrides: Partial<BaseProps> = {},
  empty: EmptyChoice = { emptyKind: 'transactions' }
) {
  const props = { ...baseProps(), ...overrides };
  return {
    props,
    ...renderWithProviders(<DataList<Row> {...props} {...empty} />),
  };
}

/**
 * The HTML DataList renders for its body (the element before the pager).
 * Compared against a reference render of the expected state component, so
 * the assertion does not depend on that component's internal DOM.
 */
function bodyHtml(ui: ReactElement): string {
  const { container, unmount } = renderWithProviders(ui);
  const html = container.firstElementChild?.innerHTML ?? '';
  unmount();
  return html;
}

function referenceHtml(ui: ReactElement): string {
  const { container, unmount } = renderWithProviders(ui);
  const html = container.innerHTML;
  unmount();
  return html;
}

describe('DataList', () => {
  it('renders the table and the pager, with no frame of its own', () => {
    const { props, container } = renderList();
    expect(screen.getByText('row-a')).toBeInTheDocument();
    expect(screen.getByText('Latest results')).toBeInTheDocument();
    expect(container.querySelector('.MuiCard-root')).toBeNull();
    expect(screen.getByRole('button', { name: 'Previous' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    expect(props.pager.handleNext).toHaveBeenCalledTimes(1);
  });

  it('renders filters, body and pager in that order', () => {
    renderList({ filters: <div>filters</div> });
    const filters = screen.getByText('filters');
    const row = screen.getByText('row-a');
    const pager = screen.getByText('Latest results');
    expect(
      filters.compareDocumentPosition(row) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy();
    expect(
      row.compareDocumentPosition(pager) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy();
  });

  it('renders the table in loading mode, without rows, while loading or reloading', () => {
    const { unmount } = renderList({ query: query({ isLoading: true }) });
    expect(screen.getByText('skeleton (0 rows)')).toBeInTheDocument();
    expect(screen.queryByText('row-a')).not.toBeInTheDocument();
    unmount();

    // Reloading keeps the previous rows around; they must not show.
    renderList({ query: query({ isPlaceholderData: true }) });
    expect(screen.getByText('skeleton (0 rows)')).toBeInTheDocument();
    expect(screen.queryByText('row-a')).not.toBeInTheDocument();
    // The pager stays mounted under the skeleton.
    expect(screen.getByText('Latest results')).toBeInTheDocument();
  });

  it('renders the error state with a working retry', () => {
    const q = query({ isError: true, error: new Error('boom') });
    renderList({ query: q });
    expect(screen.getByText('Something went wrong')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(q.refetch).toHaveBeenCalledTimes(1);
    // Called bare, never with the click event as refetch options.
    expect(q.refetch).toHaveBeenCalledWith();
  });

  it('pads the error state at py 8 unless errorPy overrides it', () => {
    const error = new Error('boom');
    const base = { ...baseProps(), query: query({ isError: true, error }) };
    // DataList always wires the retry (to `query.refetch`), so the reference
    // renders with one too.
    const retry = () => undefined;
    const py8 = referenceHtml(
      <QueryErrorState error={error} onRetry={retry} py={8} />
    );
    const py6 = referenceHtml(
      <QueryErrorState error={error} onRetry={retry} py={6} />
    );
    expect(py6).not.toBe(py8);

    expect(bodyHtml(<DataList<Row> {...base} emptyKind="transactions" />)).toBe(
      py8
    );
    expect(
      bodyHtml(<DataList<Row> {...base} emptyKind="transactions" errorPy={6} />)
    ).toBe(py6);
  });

  it('renders the emptyKind preset at its own padding unless emptyPy overrides it', () => {
    const base = { ...baseProps(), pager: pager({ rows: [] }) };
    const preset = referenceHtml(<TableEmptyState kind="transactions" />);
    const py6 = referenceHtml(<TableEmptyState kind="transactions" py={6} />);
    expect(py6).not.toBe(preset);

    expect(bodyHtml(<DataList<Row> {...base} emptyKind="transactions" />)).toBe(
      preset
    );
    expect(
      bodyHtml(<DataList<Row> {...base} emptyKind="transactions" emptyPy={6} />)
    ).toBe(py6);
  });

  it('renders renderEmpty instead of the preset', () => {
    renderList(
      { pager: pager({ rows: [] }) },
      { renderEmpty: () => <div>custom empty</div> }
    );
    expect(screen.getByText('custom empty')).toBeInTheDocument();
    expect(screen.queryByText('No transactions yet')).not.toBeInTheDocument();
  });

  it('prefers the filtered-empty state over renderEmpty when filters are active', () => {
    const onClearFilters = vi.fn();
    renderWithProviders(
      <DataList<Row>
        {...baseProps()}
        pager={pager({ rows: [] })}
        hasActiveFilters
        emptyNoun="transactions"
        onClearFilters={onClearFilters}
        renderEmpty={() => <div>custom empty</div>}
      />
    );
    expect(
      screen.getByText('No transactions match your filters')
    ).toBeInTheDocument();
    expect(screen.queryByText('custom empty')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Clear filters' }));
    expect(onClearFilters).toHaveBeenCalledTimes(1);
  });

  it('takes exactly one of emptyKind / renderEmpty, emptyPy only with emptyKind, emptyNoun with filters', () => {
    const base = baseProps();
    const empty = () => null;
    // These never render: the checks are the `@ts-expect-error`s, which fail
    // `typecheck` if the prop union ever stops rejecting the call.
    const invalid = [
      () => (
        // @ts-expect-error — `emptyKind` and `renderEmpty` are exclusive.
        <DataList<Row> {...base} emptyKind="transactions" renderEmpty={empty} />
      ),
      () => (
        // @ts-expect-error — one of `emptyKind` / `renderEmpty` is required.
        <DataList<Row> {...base} />
      ),
      () => (
        // @ts-expect-error — `emptyPy` pads the preset; meaningless here.
        <DataList<Row> {...base} renderEmpty={empty} emptyPy={6} />
      ),
      () => (
        // @ts-expect-error — a filtered list must name what it lists.
        <DataList<Row> {...base} emptyKind="transactions" hasActiveFilters />
      ),
    ];
    expect(invalid).toHaveLength(4);
  });
});
