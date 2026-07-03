import { Box, Card, Skeleton, Stack, Typography } from '@mui/material';
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
export function ListPageSkeleton({
  columns = 5,
  showFilters = true,
  rowHeight,
}: {
  columns?: number;
  showFilters?: boolean;
  /** Match the mounted page's table `rowHeight` (tall tables: NFTs / pools /
   *  ledgers) so phase A → phase B has no height jump. */
  rowHeight?: number;
}) {
  return (
    <Stack spacing={3}>
      {/* PageHeader placeholder (title + subtitle bars) */}
      <Stack spacing={1}>
        <Typography variant="heading5SemiBold" component="h1">
          <Skeleton variant="text" width={220} />
        </Typography>
        <Typography variant="bodyMedium">
          <Skeleton variant="text" width={340} />
        </Typography>
      </Stack>

      <Card>
        {/* Filter-bar placeholder — omitted for lists with no filters (e.g.
            ledgers) so the table doesn't sit ~72px too low vs loaded. */}
        {showFilters && (
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
              sx={(theme) => ({
                flexGrow: 1,
                minWidth: 200,
                borderRadius: `${theme.shape.radius.s}px`,
              })}
            />
            <Skeleton
              variant="rounded"
              width={150}
              height={40}
              sx={(theme) => ({ borderRadius: `${theme.shape.radius.s}px` })}
            />
          </Box>
        )}

        {/* Table body — same flush TableSkeleton DataListCard renders in phase B
            (row count == DataListCard's `skeletonRows` default = a full
            PAGE_SIZE page, so phase A → phase B has no height jump). */}
        <TableSkeleton rows={20} columns={columns} rowHeight={rowHeight} />

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
