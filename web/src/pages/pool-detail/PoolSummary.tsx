import { Box, Stack, Typography } from '@mui/material';
import type { PoolItem } from '@rumblefish/api-types';
import {
  formatAmount,
  IdentifierDisplay,
  IdentifierWithCopy,
} from '@rumblefish/soroban-block-explorer-ui';

import { SectionCard } from '../detail/SectionCard.js';
import { SummaryRow, SummaryRows } from '../detail/SummaryRow.js';

import {
  isSorobanPool,
  poolLegViews,
  type PoolLegView,
} from '../pool-shared/helpers.js';

interface AssetReserveCellProps {
  leg: PoolLegView;
}

function AssetReserveCell({ leg }: AssetReserveCellProps) {
  const codeNode = leg.href ? (
    <IdentifierDisplay
      value={leg.label}
      type="asset"
      truncate={false}
      href={leg.href}
      fontSize={12}
    />
  ) : (
    <Typography component="span" variant="bodyXsMedium">
      {leg.label}
    </Typography>
  );

  return (
    <Stack direction="row" spacing={1} alignItems="center">
      <Box
        component="span"
        sx={{
          display: 'inline-block',
          width: 8,
          height: 8,
          borderRadius: '50%',
          bgcolor: leg.dotColor,
          flexShrink: 0,
        }}
      />
      <Typography component="span" variant="bodyXsMedium">
        {/* `leg.reserve` is display-ready in BOTH worlds: classic amounts
            arrive pre-scaled (Decimal128(7) → string), soroban raw units are
            scaled by the leg's on-chain decimals inside `poolLegViews` —
            and an unknown scale is null, never a raw integer. */}
        {leg.reserve != null ? formatAmount(leg.reserve) : '—'}
      </Typography>
      {leg.reserve != null ? codeNode : null}
    </Stack>
  );
}

interface PoolSummaryProps {
  pool: PoolItem;
}

/**
 * "Summary" key-value card on the LP detail page (Figma node `325:7192`).
 * Row layout:
 *
 *   • Pool ID — full strkey (`L...` classic / `C...` soroban), copyable
 *   • Fee % (left) │ Total shares (right)
 *   • [soroban] Protocol (left) │ Pool type (right)
 *   • Leg reserves (dot), two per row — 1 row for pairs, 2 for 3/4 legs
 */
export function PoolSummary({ pool }: PoolSummaryProps) {
  const legs = poolLegViews(pool);
  const soroban = isSorobanPool(pool);

  return (
    <SectionCard title="Summary">
      <SummaryRow
        cells={[
          {
            label: 'Pool ID',
            value: (
              <IdentifierWithCopy
                value={pool.pool_id}
                type={soroban ? 'contract' : 'pool'}
                linked={false}
                truncate={false}
              />
            ),
          },
        ]}
      />
      <SummaryRow
        cells={[
          { label: 'Fee', value: `${formatAmount(pool.fee_percent, 2)}%` },
          {
            label: 'Total shares',
            value:
              pool.total_shares != null ? formatAmount(pool.total_shares) : '—',
          },
        ]}
      />
      {soroban && (
        <SummaryRow
          cells={[
            {
              // Verified-operator label only (task 0374 T1); an unlabelled
              // router renders an explicit em-dash, never a guess.
              label: 'Protocol',
              value: pool.protocol ?? '—',
            },
            {
              label: 'Pool type',
              value: pool.pool_type ?? '—',
            },
          ]}
        />
      )}
      <SummaryRows
        cells={legs.map((leg) => ({
          label: `${leg.label} reserve`,
          value: <AssetReserveCell leg={leg} />,
        }))}
      />
    </SectionCard>
  );
}
