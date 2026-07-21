import KeyboardArrowDownIcon from '@mui/icons-material/KeyboardArrowDown';
import {
  Box,
  ButtonBase,
  Skeleton,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
} from '@mui/material';
import { type ReactNode } from 'react';

/**
 * Fixed body-row height (px). Single source of truth shared with
 * `TableSkeleton` so the loading skeleton is pixel-for-pixel the same height as
 * the populated table — no layout jump when swapping data ↔ skeleton on
 * pagination / filter changes.
 */
export const EXPLORER_TABLE_ROW_HEIGHT = 44;

/**
 * Row height (px) for tables whose cells render two-line content (e.g. the
 * two-line `TransactionTime`), a 40px media thumbnail (NFTs), or a stacked
 * asset pair (liquidity pools). These rows are taller than the single-line
 * default; pinning them to one value (≥ the tallest cell content, so nothing
 * clips) keeps every such row uniform AND lets the loading skeleton match the
 * populated table pixel-for-pixel — no layout jump on data ↔ skeleton.
 */
export const EXPLORER_TABLE_ROW_HEIGHT_TALL = 56;

export type SortDirection = 'asc' | 'desc';

export interface ExplorerTableColumn<T> {
  id: string;
  header: ReactNode;
  align?: 'left' | 'right' | 'center';
  width?: number | string;
  sortable?: boolean;
  cell: (row: T, index: number) => ReactNode;
}

export interface ExplorerTableProps<T> {
  columns: ExplorerTableColumn<T>[];
  rows: readonly T[];
  rowKey: (row: T, index: number) => string;
  sortBy?: string;
  sortDir?: SortDirection;
  onSortChange?: (id: string, dir: SortDirection) => void;
  emptyState?: ReactNode;
  /**
   * Fixed body-row height (px). Defaults to the single-line
   * [`EXPLORER_TABLE_ROW_HEIGHT`]; pass [`EXPLORER_TABLE_ROW_HEIGHT_TALL`] for
   * tables with two-line / media cells so every row is uniform.
   */
  rowHeight?: number;
  /**
   * Render the loading skeleton INSTEAD of `rows`: the real `<TableHead>` plus
   * `skeletonRows` placeholder body rows, in this exact same table / container /
   * column layout. Reusing the real structure (not a separate skeleton
   * component) is what makes the skeleton the same height as the populated
   * table at EVERY viewport — headers wrap identically, the horizontal-scroll
   * container behaves identically — so there is no layout jump on data ↔
   * skeleton, responsively.
   */
  loading?: boolean;
  /** Placeholder row count while `loading`. */
  skeletonRows?: number;
}

interface SortableHeaderProps {
  label: ReactNode;
  isSorted: boolean;
  direction: SortDirection;
  align: 'left' | 'right' | 'center';
  onClick: () => void;
}

function SortableHeader({
  label,
  isSorted,
  direction,
  align,
  onClick,
}: SortableHeaderProps) {
  return (
    <ButtonBase
      onClick={onClick}
      sx={(theme) => ({
        display: 'inline-flex',
        alignItems: 'center',
        gap: 0.75,
        justifyContent:
          align === 'right'
            ? 'flex-end'
            : align === 'center'
            ? 'center'
            : 'flex-start',
        color: theme.palette.text.primary,
        '&:focus-visible': {
          outline: `2px solid ${theme.palette.stroke.action}`,
          outlineOffset: 2,
          borderRadius: `${theme.shape.radius.xs}px`,
        },
      })}
    >
      {label}
      <Box
        component="span"
        sx={(theme) => ({
          display: 'inline-flex',
          alignItems: 'center',
          justifyContent: 'center',
          width: 20,
          height: 20,
          borderRadius: '50%',
          flexShrink: 0,
          backgroundColor: isSorted
            ? theme.palette.surface.primaryMain
            : theme.palette.surface.grayLight,
          color: isSorted
            ? theme.palette.common.black
            : theme.palette.text.tertiary,
          transition: 'background-color 0.15s, color 0.15s',
        })}
      >
        <KeyboardArrowDownIcon
          sx={{
            fontSize: 14,
            transition: 'transform 150ms ease',
            transform: direction === 'asc' ? 'rotate(180deg)' : 'rotate(0deg)',
          }}
        />
      </Box>
    </ButtonBase>
  );
}

export function ExplorerTable<T>({
  columns,
  rows,
  rowKey,
  sortBy,
  sortDir = 'desc',
  onSortChange,
  emptyState,
  rowHeight = EXPLORER_TABLE_ROW_HEIGHT,
  loading = false,
  skeletonRows = 10,
}: ExplorerTableProps<T>) {
  const isEmpty = rows.length === 0;

  // `tableLayout: fixed` → column widths come from the per-column `width`s, NOT
  // the cell content. This makes the loading skeleton's columns identical to the
  // populated table's (content no longer shifts the columns) AND lets columns
  // keep a content-sized PIXEL width that never compresses below the data. The
  // table's `minWidth` is the sum of those pixel widths, so on a narrow screen
  // the container scrolls horizontally instead of squeezing/truncating cells
  // (cols are sized in px in each table; falls back to a sane default if a
  // table only uses % / no widths).
  const minWidth =
    columns.reduce(
      (sum, c) => sum + (typeof c.width === 'number' ? c.width : 0),
      0
    ) || 720;

  // Drop rows whose `rowKey` was already seen, keeping the first.
  //
  // Callers key rows by a domain id they believe is unique (`sequence`, `id`, a
  // hash). When the backend hands back a duplicate — which a ReplacingMergeTree
  // read without deduplication does (lore-0420) — React sees two siblings with
  // the same key, cannot match old nodes to new ones, and leaves orphans behind:
  // the table APPENDED rows without bound on every re-sort. Rendering one row per
  // key removes that failure mode at the source, since the collision never
  // reaches React.
  //
  // Backend correctness is the real fix (the rest of lore-0420); this only keeps
  // a data fault from escalating into an unbounded rendering fault. A duplicate
  // is still a backend bug, so it is reported to the console rather than passed
  // over in silence — the list would otherwise look perfectly healthy while the
  // query behind it was wrong.
  const seenKeys = new Set<string>();
  const visibleRows: { row: T; key: string }[] = [];
  rows.forEach((row, idx) => {
    const key = rowKey(row, idx);
    if (seenKeys.has(key)) return;
    seenKeys.add(key);
    visibleRows.push({ row, key });
  });
  if (visibleRows.length !== rows.length) {
    // eslint-disable-next-line no-console
    console.warn(
      `ExplorerTable: dropped ${
        rows.length - visibleRows.length
      } row(s) with a duplicate rowKey. ` +
        `The query feeding this table is returning duplicates — see lore-0420.`
    );
  }

  return (
    <TableContainer
      sx={{
        overflowX: 'auto',
      }}
    >
      <Table sx={{ tableLayout: 'fixed', minWidth }}>
        <TableHead
          sx={(theme) => ({
            backgroundColor: theme.palette.surface.backgroundAlt,
          })}
        >
          <TableRow>
            {columns.map((col) => {
              const isSorted = sortBy === col.id;
              return (
                <TableCell
                  key={col.id}
                  align={col.align ?? 'left'}
                  width={col.width}
                  sortDirection={isSorted ? sortDir : false}
                  sx={{
                    whiteSpace: 'nowrap',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                  }}
                >
                  {col.sortable ? (
                    <SortableHeader
                      label={col.header}
                      isSorted={isSorted}
                      direction={isSorted ? sortDir : 'desc'}
                      align={col.align ?? 'left'}
                      onClick={() => {
                        const next: SortDirection =
                          isSorted && sortDir === 'desc' ? 'asc' : 'desc';
                        onSortChange?.(col.id, next);
                      }}
                    />
                  ) : (
                    col.header
                  )}
                </TableCell>
              );
            })}
          </TableRow>
        </TableHead>
        <TableBody>
          {loading
            ? // Skeleton placeholder rows in the EXACT same row/cell layout as
              // the data rows (same `rowHeight`, same `py`, same column widths),
              // under the real `<TableHead>` above — so the loading state is the
              // same height as the populated table at every viewport.
              Array.from({ length: skeletonRows }).map((_, r) => (
                <TableRow
                  key={`skeleton-${r}`}
                  // Same alternating row background as the data rows below, so
                  // the skeleton looks like the populated table (not a flat block).
                  sx={(theme) => ({
                    height: rowHeight,
                    backgroundColor:
                      r % 2 === 1
                        ? theme.palette.surface.grayMainAlt
                        : theme.palette.surface.grayMain,
                  })}
                  data-testid="explorer-table-skeleton-row"
                >
                  {columns.map((col, c) => (
                    <TableCell
                      key={col.id}
                      align={col.align ?? 'left'}
                      width={col.width}
                      sx={{
                        py: 0.5,
                        whiteSpace: 'nowrap',
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                      }}
                    >
                      {/* `inline-block` so the bar follows the cell's
                          `text-align` (= the column `align`): right-aligned
                          columns (balances, counts) get the bar on the right,
                          where the real data sits — not stuck on the left. */}
                      <Skeleton
                        variant="text"
                        width={c === 0 ? '70%' : '50%'}
                        sx={{ display: 'inline-block' }}
                      />
                    </TableCell>
                  ))}
                </TableRow>
              ))
            : isEmpty
            ? // No `emptyState` → render nothing rather than an empty 96px-tall
              // placeholder row. Callers that want a placeholder pass one in.
              emptyState !== undefined && (
                <TableRow>
                  <TableCell
                    colSpan={columns.length}
                    sx={{ borderBottom: 'none' }}
                  >
                    <Box
                      sx={{
                        display: 'flex',
                        justifyContent: 'center',
                        py: 6,
                      }}
                    >
                      {emptyState}
                    </Box>
                  </TableCell>
                </TableRow>
              )
            : visibleRows.map(({ row, key }, idx) => (
                <TableRow
                  key={key}
                  sx={(theme) => ({
                    backgroundColor:
                      idx % 2 === 1
                        ? theme.palette.surface.grayMainAlt
                        : theme.palette.surface.grayMain,

                    height: rowHeight,
                  })}
                >
                  {columns.map((col) => (
                    <TableCell
                      key={col.id}
                      align={col.align ?? 'left'}
                      width={col.width}
                      sx={{
                        py: 0.5,
                        whiteSpace: 'nowrap',
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                      }}
                    >
                      {col.cell(row, idx)}
                    </TableCell>
                  ))}
                </TableRow>
              ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}
