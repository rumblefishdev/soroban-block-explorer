import {
  Box,
  Skeleton,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
} from '@mui/material';

import { EXPLORER_TABLE_ROW_HEIGHT } from '../../table/ExplorerTable.js';

interface TableSkeletonProps {
  rows?: number;
  columns?: number;
  showHeader?: boolean;
  /**
   * Body-row height (px) — MUST match the populated table's `rowHeight` so the
   * skeleton is the same height (no jump). Defaults to the single-line
   * [`EXPLORER_TABLE_ROW_HEIGHT`]; pass [`EXPLORER_TABLE_ROW_HEIGHT_TALL`] for
   * two-line / media tables.
   */
  rowHeight?: number;
}

export function TableSkeleton({
  rows = 5,
  columns = 4,
  showHeader = true,
  rowHeight = EXPLORER_TABLE_ROW_HEIGHT,
}: TableSkeletonProps) {
  return (
    <Box sx={{ width: '100%' }}>
      <Table>
        {showHeader && (
          <TableHead>
            <TableRow>
              {Array.from({ length: columns }).map((_, i) => (
                <TableCell key={i}>
                  <Skeleton variant="text" width="60%" />
                </TableCell>
              ))}
            </TableRow>
          </TableHead>
        )}
        <TableBody>
          {Array.from({ length: rows }).map((_, r) => (
            // Match `ExplorerTable` body rows exactly (`height` + cell `py: 0.5`)
            // so the skeleton is the same height as the populated table — no
            // layout jump on the data ↔ skeleton swap.
            <TableRow key={r} sx={{ height: rowHeight }}>
              {Array.from({ length: columns }).map((_, c) => (
                <TableCell key={c} sx={{ py: 0.5 }}>
                  <Skeleton variant="text" width={c === 0 ? '70%' : '50%'} />
                </TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </Box>
  );
}
