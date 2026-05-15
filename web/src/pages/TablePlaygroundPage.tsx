import { Box, Paper, Stack, Typography } from '@mui/material';
import {
  Chip,
  ExplorerTable,
  PaginationControls,
  RelativeTimestamp,
  TableEmptyState,
  TableSectionHeader,
  useTableUrlState,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import { useMemo } from 'react';

/**
 * Visual playground for the task 0061 table primitives. Renders the
 * components against mock data so 1:1 parity with the Figma design can
 * be checked in a browser. Not a production route — drop once real list
 * pages (separate tasks) exercise the same components.
 */

interface MockTx {
  hash: string;
  sourceAccount: string;
  operation: string;
  status: 'success' | 'failed';
  closedAt: string;
}

const MINUTE = 60_000;
const now = Date.now();

const MOCK_ROWS: MockTx[] = [
  {
    hash: 'a1b2c3d4e5f6...f8e9d0c1',
    sourceAccount: 'GABC...XYZ1',
    operation: 'Payment',
    status: 'success',
    closedAt: new Date(now - 2 * MINUTE).toISOString(),
  },
  {
    hash: 'c3d4e5f6g7h8...a9b8c7d6',
    sourceAccount: 'GDEF...XYZ2',
    operation: 'Invoke Host Function',
    status: 'success',
    closedAt: new Date(now - 6 * MINUTE).toISOString(),
  },
  {
    hash: 'e5f6g7h8i9j0...e1f2a3b4',
    sourceAccount: 'GHIJ...XYZ3',
    operation: 'Create Account',
    status: 'failed',
    closedAt: new Date(now - 11 * MINUTE).toISOString(),
  },
  {
    hash: 'g7h8i9j0k1l2...c5d6e7f8',
    sourceAccount: 'GKLM...XYZ4',
    operation: 'Change Trust',
    status: 'success',
    closedAt: new Date(now - 29 * MINUTE).toISOString(),
  },
  {
    hash: 'i9j0k1l2m3n4...a7b8c9d0',
    sourceAccount: 'GNOP...XYZ5',
    operation: 'Manage Sell Offer',
    status: 'success',
    closedAt: new Date(now - 62 * MINUTE).toISOString(),
  },
];

export default function TablePlaygroundPage() {
  const { state, setSort, setCursor } = useTableUrlState({
    defaultSortBy: 'closedAt',
    defaultSortDir: 'desc',
  });

  const rows = useMemo(() => {
    const sorted = [...MOCK_ROWS];
    if (state.sortBy === 'closedAt') {
      sorted.sort((a, b) =>
        state.sortDir === 'asc'
          ? a.closedAt.localeCompare(b.closedAt)
          : b.closedAt.localeCompare(a.closedAt)
      );
    }
    return sorted;
  }, [state.sortBy, state.sortDir]);

  const columns: ExplorerTableColumn<MockTx>[] = [
    {
      id: 'hash',
      header: 'Hash',
      cell: (row) => (
        <Typography variant="bodyMonoSmRegular">{row.hash}</Typography>
      ),
    },
    {
      id: 'sourceAccount',
      header: 'Source account',
      cell: (row) => (
        <Typography variant="bodyMonoSmRegular">{row.sourceAccount}</Typography>
      ),
    },
    {
      id: 'operation',
      header: 'Operation',
      cell: (row) => row.operation,
    },
    {
      id: 'status',
      header: 'Status',
      cell: (row) => (
        <Chip
          label={row.status === 'success' ? 'Success' : 'Failed'}
          color={row.status === 'success' ? 'emerald' : 'error'}
          size="sm"
        />
      ),
    },
    {
      id: 'closedAt',
      header: 'Time',
      align: 'right',
      sortable: true,
      cell: (row) => <RelativeTimestamp timestamp={row.closedAt} />,
    },
  ];

  return (
    <Stack spacing={4} sx={{ py: 2 }}>
      <Box>
        <Typography variant="heading3SemiBold">Table playground</Typography>
        <Typography variant="bodyRegular" sx={{ color: 'text.tertiary' }}>
          Mock data — visual check for task 0061 primitives.
        </Typography>
      </Box>

      <TableCard>
        <TableSectionHeader
          title="Latest transactions"
          badge={<Chip label="Live" color="emerald" size="sm" />}
        />
        <ExplorerTable<MockTx>
          columns={columns}
          rows={rows}
          rowKey={(row: MockTx) => row.hash}
          sortBy={state.sortBy ?? undefined}
          sortDir={state.sortDir}
          onSortChange={setSort}
        />
        <PaginationControls
          caption="Latest 5 results"
          prevCursor={state.cursor}
          nextCursor="mock-next-cursor"
          onPrev={(c: string) => setCursor(c)}
          onNext={(c: string) => setCursor(c)}
        />
      </TableCard>

      <TableCard>
        <TableSectionHeader title="Empty example" />
        <ExplorerTable<MockTx>
          columns={columns}
          rows={[]}
          rowKey={(row: MockTx) => row.hash}
          emptyState={<TableEmptyState kind="transactions" />}
        />
      </TableCard>
    </Stack>
  );
}

function TableCard({ children }: { children: React.ReactNode }) {
  return (
    <Paper
      elevation={0}
      sx={(theme) => ({
        border: `1px solid ${theme.palette.stroke.default}`,
        borderRadius: `${theme.shape.radius.md}px`,
        overflow: 'hidden',
      })}
    >
      {children}
    </Paper>
  );
}
