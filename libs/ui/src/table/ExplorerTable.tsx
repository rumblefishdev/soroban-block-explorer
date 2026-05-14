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
        <TableHead>
          <TableRow>
            {columns.map((col) => {
              const active = sortBy === col.id;
              const dir = active ? sortDir : 'desc';
              return (
                <TableCell
                  key={col.id}
                  align={col.align ?? 'left'}
                  width={col.width}
                  sortDirection={active ? sortDir : false}
                >
                  {col.sortable ? (
                    <TableSortLabel
                      active={active}
                      direction={dir}
                      onClick={() => {
                        const next: SortDirection =
                          active && sortDir === 'desc' ? 'asc' : 'desc';
                        onSortChange?.(col.id, next);
                      }}
                      IconComponent={
                        active ? KeyboardArrowDownIcon : UnfoldMoreIcon
                      }
                      sx={(theme) => ({
                        ...(active && {
                          '& .MuiTableSortLabel-icon': {
                            color: theme.palette.text.primary,
                            backgroundColor: theme.palette.surface.primaryMain,
                            borderRadius: theme.shape.radius.pills,
                            padding: '2px',
                            fontSize: 20,
                            opacity: 1,
                          },
                        }),
                        '&:not(.Mui-active) .MuiTableSortLabel-icon': {
                          opacity: 0.5,
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
