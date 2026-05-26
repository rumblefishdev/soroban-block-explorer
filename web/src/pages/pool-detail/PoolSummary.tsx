import { Box, Link, Stack, Typography } from '@mui/material';
import type { PoolAssetLeg, PoolItem } from '@rumblefish/api-types';
import { IdentifierWithCopy } from '@rumblefish/soroban-block-explorer-ui';
import { Link as RouterLink } from 'react-router-dom';

import { routes } from '../../router/routes.js';
import { SectionCard } from '../detail/SectionCard.js';
import { SummaryRow } from '../detail/SummaryRow.js';
import { formatAmount } from '../format.js';

import { assetLegLabel } from './helpers.js';

/**
 * Resolve the cross-entity link target for a pool asset leg (task 0263).
 * Always routes to the **asset detail page** — that is the natural target
 * when the user clicks the asset code on a pool reserve cell. Backend
 * `parse_asset_id` accepts either the SAC C-strkey or a `code-issuer`
 * composite, so both classic and SAC legs resolve to the same asset row.
 *
 * Precedence:
 *   1. `asset_type === 0` (native XLM) → no link. Stellar native has no
 *      on-chain address in classic protocol and `parse_asset_id` does
 *      not accept a `native` alias; the SAC mirror for XLM is also
 *      network-dependent, so we don't fabricate a target.
 *   2. `contract_id` (SAC mirror) → `/assets/${contract_id}` (C-strkey
 *      form — canonical for SAC / Soroban tokens).
 *   3. `asset_code` + `issuer` (classic credit, no SAC mirror) →
 *      `/assets/${asset_code}-${issuer}` (composite form).
 *   4. Anything else (schema drift) → no link.
 */
function legHref(leg: PoolAssetLeg): string | undefined {
  if (leg.asset_type === 0) return undefined;
  if (leg.contract_id) return routes.asset(leg.contract_id);
  if (leg.asset_code && leg.issuer) {
    return routes.asset(`${leg.asset_code}-${leg.issuer}`);
  }
  return undefined;
}

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
      variant="bodySmRegular"
      sx={{ color: 'text.primary' }}
    >
      {code}
    </Link>
  ) : (
    <Typography component="span" variant="bodySmRegular">
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
      <Typography component="span" variant="bodySmRegular">
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
                dotColor="primary.main"
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
                dotColor="success.main"
                href={legHref(pool.asset_b)}
              />
            ),
          },
        ]}
      />
    </SectionCard>
  );
}
