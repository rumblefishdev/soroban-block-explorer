import KeyboardArrowDownIcon from '@mui/icons-material/KeyboardArrowDown';
import UnfoldMoreIcon from '@mui/icons-material/UnfoldMore';
import {
  Box,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TableSortLabel,
} from '@mui/material';
import type { ReactNode } from 'react';

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
}

export function ExplorerTable<T>({
  columns,
  rows,
  rowKey,
  sortBy,
  sortDir = 'desc',
  onSortChange,
  emptyState,
}: ExplorerTableProps<T>) {
  const isEmpty = rows.length === 0;
  return (
    <TableContainer>
      <Table>
        <TableHead
          sx={(theme) => ({
            // Table header sits on the darkest surface (Figma).
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
                    // Unsorted columns show a neutral CaretUpDown; the
                    // sorted column swaps to a directional caret that
                    // MUI rotates for asc/desc.
                    <TableSortLabel
                      active={isSorted}
                      direction={isSorted ? sortDir : 'desc'}
                      onClick={() => {
                        const next: SortDirection =
                          isSorted && sortDir === 'desc' ? 'asc' : 'desc';
                        onSortChange?.(col.id, next);
                      }}
                      IconComponent={
                        isSorted ? KeyboardArrowDownIcon : UnfoldMoreIcon
                      }
                      sx={(theme) => ({
                        '& .MuiTableSortLabel-icon': {
                          fontSize: 12,
                          opacity: isSorted ? 1 : 0.5,
                          color: theme.palette.text.primary,
                        },
                      })}
                    >
                      {col.header}
                    </TableSortLabel>
                  ) : (
                    col.header
                  )}
                </TableCell>
              );
            })}
          </TableRow>
        </TableHead>
        <TableBody>
          {isEmpty ? (
            <TableRow>
              <TableCell colSpan={columns.length} sx={{ borderBottom: 'none' }}>
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
          ) : (
            rows.map((row, idx) => (
              <TableRow
                key={rowKey(row, idx)}
                sx={(theme) => ({
                  backgroundColor:
                    idx % 2 === 1
                      ? theme.palette.surface.grayMainAlt
                      : theme.palette.surface.grayMain,
                })}
              >
                {columns.map((col) => (
                  <TableCell
                    key={col.id}
                    align={col.align ?? 'left'}
                    width={col.width}
                    // Fixed 48px row, per the Design System table cell.
                    sx={{ height: 48, py: 0 }}
                  >
                    {col.cell(row, idx)}
                  </TableCell>
                ))}
              </TableRow>
            ))
          )}
        </TableBody>
      </Table>
    </TableContainer>
  );
}
