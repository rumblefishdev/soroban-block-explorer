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
        <TableHead>
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
                    // Sortable header (Design System "Table header"):
                    // a neutral up/down caret when inactive, a down caret
                    // in a filled accent circle when this column is sorted.
                    <Box
                      component="button"
                      type="button"
                      // `aria-sort` lives on the parent `<th>` (TableCell
                      // `sortDirection`); the button only needs an action label.
                      aria-label={`Sort by ${col.id}`}
                      onClick={() => {
                        const next: SortDirection =
                          isSorted && sortDir === 'desc' ? 'asc' : 'desc';
                        onSortChange?.(col.id, next);
                      }}
                      sx={{
                        display: 'inline-flex',
                        alignItems: 'center',
                        gap: 0.5,
                        border: 0,
                        background: 'none',
                        padding: 0,
                        font: 'inherit',
                        color: 'inherit',
                        cursor: 'pointer',
                        '&:focus-visible': {
                          outline: (theme) =>
                            `2px solid ${theme.palette.stroke.action}`,
                          outlineOffset: 2,
                          borderRadius: 1,
                        },
                      }}
                    >
                      {col.header}
                      {isSorted ? (
                        <Box
                          component="span"
                          sx={{
                            width: 20,
                            height: 20,
                            flexShrink: 0,
                            borderRadius: '50%',
                            display: 'inline-flex',
                            alignItems: 'center',
                            justifyContent: 'center',
                            backgroundColor: 'surface.primaryMain',
                            color: 'common.black',
                          }}
                        >
                          <KeyboardArrowDownIcon
                            sx={{
                              fontSize: 14,
                              transform:
                                sortDir === 'asc' ? 'rotate(180deg)' : 'none',
                            }}
                          />
                        </Box>
                      ) : (
                        <UnfoldMoreIcon
                          sx={{
                            fontSize: 14,
                            opacity: 0.5,
                            color: 'text.primary',
                          }}
                        />
                      )}
                    </Box>
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
