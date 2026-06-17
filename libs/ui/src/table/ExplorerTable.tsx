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
import { type ReactNode } from 'react';

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
}: ExplorerTableProps<T>) {
  const isEmpty = rows.length === 0;

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
