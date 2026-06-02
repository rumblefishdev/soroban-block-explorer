import { Box, Card, Skeleton, Stack } from '@mui/material';
import { TableSkeleton } from '@rumblefish/soroban-block-explorer-ui';

/**
 * Route-level Suspense fallback for the six list pages (transactions,
 * ledgers, accounts, assets, nfts, liquidity-pools). Mirrors the shared
 * list shell — `PageHeader` + `DataListCard` (filters + table +
 * pagination) — so the lazy-chunk fallback (phase A) matches the mounted
 * page's own loading state (phase B), where `DataListCard` renders the same
 * `TableSkeleton`. Kills the card→table layout flicker the generic
 * `DetailSkeleton` fallback caused (F-W6-LOADSKEL-2).
 *
 * Eager + self-contained (only libs/ui primitives, no lazy-page imports) so
 * it can be referenced by the router without pulling a page chunk into the
 * main bundle. `columns` is approximate — skeleton bars, not pixel-exact.
 */
export function ListPageSkeleton({ columns = 5 }: { columns?: number }) {
  return (
    <Stack spacing={3}>
      {/* PageHeader placeholder: title + subtitle */}
      <Stack spacing={1}>
        <Skeleton variant="text" width={220} height={32} />
        <Skeleton variant="text" width={340} height={20} />
      </Stack>

      <Card>
        {/* Filter-bar placeholder */}
        <Box
          sx={{
            p: 2,
            display: 'flex',
            gap: 1.5,
            flexWrap: 'wrap',
            alignItems: 'center',
          }}
        >
          <Skeleton
            variant="rounded"
            height={40}
            sx={{ flexGrow: 1, minWidth: 200, borderRadius: '8px' }}
          />
          <Skeleton
            variant="rounded"
            width={150}
            height={40}
            sx={{ borderRadius: '8px' }}
          />
        </Box>

        {/* Table body — same TableSkeleton DataListCard renders in phase B */}
        <Box sx={{ p: 2 }}>
          <TableSkeleton rows={10} columns={columns} />
        </Box>

        {/* Pagination placeholder */}
        <Box
          sx={{
            px: 2,
            py: 1.5,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            borderTop: (theme) => `1px solid ${theme.palette.stroke.default}`,
          }}
        >
          <Skeleton variant="text" width={120} />
          <Stack direction="row" spacing={1}>
            <Skeleton variant="rounded" width={88} height={36} />
            <Skeleton variant="rounded" width={88} height={36} />
          </Stack>
        </Box>
      </Card>
    </Stack>
  );
}
