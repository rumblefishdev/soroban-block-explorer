import { Box, Card, Skeleton, Stack } from '@mui/material';

/**
 * Loading placeholder for a chart card (e.g. `PoolCharts`): a tabs + range
 * header bar over a chart-area box. Mirrors the real card's header strip and
 * `p:2` chart body so the chart doesn't pop in / shift on load.
 */
export function ChartCardSkeleton() {
  return (
    <Card sx={{ p: 0 }}>
      <Box
        sx={(theme) => ({
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'flex-end',
          flexWrap: 'wrap',
          gap: 1,
          px: 2,
          py: 1,
          backgroundColor: theme.palette.surface.grayMainAlt,
          borderBottom: `1px solid ${theme.palette.stroke.default}`,
        })}
      >
        <Stack direction="row" spacing={2}>
          <Skeleton variant="text" width={48} />
          <Skeleton variant="text" width={56} />
          <Skeleton variant="text" width={40} />
        </Stack>
        <Stack direction="row" spacing={0.5}>
          {['1D', '7D', '30D', '1Y'].map((p) => (
            <Skeleton key={p} variant="rounded" width={36} height={24} />
          ))}
        </Stack>
      </Box>
      <Box sx={{ p: 2 }}>
        <Skeleton variant="rounded" height={280} sx={{ width: '100%' }} />
      </Box>
    </Card>
  );
}
