import { Stack } from '@mui/material';
import { CardSkeleton } from '@rumblefish/soroban-block-explorer-ui';
import { useParams } from 'react-router-dom';

import { PoolDetailHeader } from './PoolDetailHeader.js';

/**
 * Loading skeleton for the liquidity-pool detail page — the real (static)
 * header + KPI/summary card placeholders, matching the loaded layout. Charts
 * /participants/transactions are gated on data, so omitted while loading.
 * Used as BOTH route fallback (phase A) and the page's `isLoading` return
 * (phase B). Reuses the real `PoolDetailHeader` (takes the id as a prop, no
 * fetch) so the header is pixel-exact even in the fallback.
 */
export function PoolDetailSkeleton() {
  const { id = '' } = useParams<{ id: string }>();
  return (
    <Stack spacing={3}>
      <PoolDetailHeader poolId={id} pool={undefined} />
      <CardSkeleton />
      <CardSkeleton />
    </Stack>
  );
}
