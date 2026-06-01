import { Box, Link, Stack, Typography } from '@mui/material';
import type { PoolItem } from '@rumblefish/api-types';
import { IdentifierWithCopy } from '@rumblefish/soroban-block-explorer-ui';
import { Link as RouterLink } from 'react-router-dom';

import { routes } from '../../router/routes.js';
import { SectionCard } from '../detail/SectionCard.js';
import { SummaryRow } from '../detail/SummaryRow.js';
import { formatAmount } from '../format.js';

import { assetLegLabel, legHref, reserveDotColor } from './helpers.js';

interface AssetReserveCellProps {
  amount: string | null | undefined;
  code: string;
  dotColor: string;
  href?: string;
}

function AssetReserveCell({
  amount,
  code,
  dotColor,
  href,
}: AssetReserveCellProps) {
  const codeNode = href ? (
    <Link
      component={RouterLink}
      to={href}
      variant="bodyXsMedium"
      sx={(theme) => ({ color: theme.palette.text.primary })}
    >
      {code}
    </Link>
  ) : (
    <Typography component="span" variant="bodyXsMedium">
      {code}
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
          bgcolor: dotColor,
          flexShrink: 0,
        }}
      />
      <Typography component="span" variant="bodyXsMedium">
        {amount != null ? formatAmount(amount) : '—'}
      </Typography>
      {amount != null ? codeNode : null}
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
 *   • Pool ID — full CAP-38 `L...` strkey, copyable, full-width row
 *   • Fee % (left) │ Total shares (right)
 *   • Asset A reserve (dot, left) │ Asset B reserve (dot, right)
 */
export function PoolSummary({ pool }: PoolSummaryProps) {
  const codeA = assetLegLabel(pool.asset_a);
  const codeB = assetLegLabel(pool.asset_b);

  return (
    <SectionCard title="Summary">
      <SummaryRow
        cells={[
          {
            label: 'Pool ID',
            value: (
              <IdentifierWithCopy
                value={pool.pool_id}
                type="pool"
                href={routes.pool(pool.pool_id)}
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
            value: formatAmount(pool.total_shares),
          },
        ]}
      />
      <SummaryRow
        cells={[
          {
            label: `${codeA} reserve`,
            value: (
              <AssetReserveCell
                amount={pool.reserve_a}
                code={codeA}
                dotColor={reserveDotColor(pool.asset_a)}
                href={legHref(pool.asset_a)}
              />
            ),
          },
          {
            label: `${codeB} reserve`,
            value: (
              <AssetReserveCell
                amount={pool.reserve_b}
                code={codeB}
                dotColor={reserveDotColor(pool.asset_b)}
                href={legHref(pool.asset_b)}
              />
            ),
          },
        ]}
      />
    </SectionCard>
  );
}
