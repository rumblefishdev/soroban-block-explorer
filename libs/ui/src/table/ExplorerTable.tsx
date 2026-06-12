import KeyboardArrowDownIcon from '@mui/icons-material/KeyboardArrowDown';
import {
  Box,
  ButtonBase,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
} from '@mui/material';
import { alpha } from '@mui/material/styles';
import { useEffect, useRef, type ReactNode } from 'react';

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
   * Live-feed mode: rows whose key was absent on the previous render get a
   * brief background flash, so an appended row reads as "new entry" rather
   * than the whole table silently swapping. The initial mount never
   * flashes. Leave off for paginated tables — a page change replaces every
   * key and would flash the entire page.
   */
  highlightNewRows?: boolean;
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
  highlightNewRows = false,
}: ExplorerTableProps<T>) {
  const isEmpty = rows.length === 0;

  // Keys visible on the previous render — `null` until the first commit so
  // the initial payload doesn't flash. Replaced (not unioned) each render:
  // in an append-feed a key absent from the previous render is, exactly, a
  // new row.
  const prevKeys = useRef<Set<string> | null>(null);
  const keys = rows.map((row, idx) => rowKey(row, idx));
  const prev = prevKeys.current;
  const freshKeys =
    highlightNewRows && prev !== null
      ? new Set(keys.filter((k) => !prev.has(k)))
      : null;
  useEffect(() => {
    if (highlightNewRows) prevKeys.current = new Set(keys);
  });

  return (
    <TableContainer
      sx={{
        overflowX: 'auto',
      }}
    >
      <Table>
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
          {isEmpty
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
            : rows.map((row, idx) => (
                <TableRow
                  key={rowKey(row, idx)}
                  sx={(theme) => ({
                    backgroundColor:
                      idx % 2 === 1
                        ? theme.palette.surface.grayMainAlt
                        : theme.palette.surface.grayMain,

                    height: 44,
                    ...(freshKeys?.has(keys[idx]) && {
                      '@media (prefers-reduced-motion: no-preference)': {
                        animation: 'explorerRowEnter 1.5s ease-out',
                      },
                      '@keyframes explorerRowEnter': {
                        from: {
                          backgroundColor: alpha(
                            theme.palette.surface.primaryMain,
                            0.16
                          ),
                        },
                      },
                    }),
                  })}
                >
                  {columns.map((col) => (
                    <TableCell
                      key={col.id}
                      align={col.align ?? 'left'}
                      width={col.width}
                      sx={{ py: 0.5 }}
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
