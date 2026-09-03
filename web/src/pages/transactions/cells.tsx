import { Box, Link, Stack, Tooltip, Typography } from '@mui/material';
import type { TransactionValue } from '@rumblefish/api-types';
import {
  Chip,
  Dash,
  formatAmount,
  IdentifierDisplay,
  IdentifierWithCopy,
  scaleByDecimals,
  StatusChip,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import { Link as RouterLink } from 'react-router-dom';

import { routes } from '../../router/routes.js';
import { isNativeAssetString, NATIVE_ASSET_CODE } from '../assets/assetType.js';

import { formatOperationType } from './operationTypes.js';

/**
 * Operation-type cell shared by every transaction table: the first operation
 * type as a chip, with a `+N` count when a transaction has more than one.
 */
export function OperationCell({ types }: { types: readonly string[] }) {
  if (types.length === 0) return <Dash />;
  const [first, ...rest] = types;
  return (
    <Box sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.5 }}>
      <Chip size="sm" color="neutral" label={formatOperationType(first)} />
      {rest.length > 0 && (
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({ color: theme.palette.text.tertiary })}
        >
          +{rest.length}
        </Typography>
      )}
    </Box>
  );
}

/**
 * "Value moved" cell (task 0393): the net-settled amount of the primary asset
 * (scaled by its decimals) with its code linking to the asset detail page, plus
 * a `+N` count when the transaction moved more than one asset. A single narrow
 * column cannot list every asset, so the rest collapse into the count. `Dash`
 * when nothing net-settled. (Per-asset breakdown on the transaction detail page
 * is a planned follow-up — not built here.)
 *
 * Pre-backfill honesty: the indexer writes `net_settled` live since ledger
 * 63,699,653 (first non-NULL row on prod, 2026-07-29); everything earlier is
 * NULL until the S3 re-ingest (task 0419) runs. An empty `values` on an older
 * transaction therefore means "not computed yet", not "nothing moved" — render
 * `n/a`, never a dash that reads as an empty value.
 */
const NET_SETTLED_LIVE_FLOOR = 63_699_653;

export function ValueCell({
  values,
  ledgerSequence,
}: {
  values: readonly TransactionValue[];
  ledgerSequence: number;
}) {
  if (values.length === 0 && ledgerSequence < NET_SETTLED_LIVE_FLOOR) {
    return (
      <Typography
        component="span"
        variant="bodySmRegular"
        sx={(theme) => ({ color: theme.palette.text.tertiary })}
      >
        n/a
      </Typography>
    );
  }
  if (values.length === 0) return <Dash />;
  const [first, ...rest] = values;
  const code = valueCode(first);
  const cell = (
    <Box sx={{ display: 'inline-flex', alignItems: 'baseline', gap: 0.5 }}>
      <Typography component="span" variant="bodySmRegular">
        {formatAmount(scaleByDecimals(first.net_settled, first.decimals), 2)}
      </Typography>
      <Link
        component={RouterLink}
        to={routes.asset(first.asset)}
        underline="hover"
        variant="bodySmRegular"
        sx={(theme) => ({ color: theme.palette.surface.primaryMainAlt })}
      >
        {code}
      </Link>
      {rest.length > 0 && (
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({ color: theme.palette.text.tertiary })}
        >
          +{rest.length}
        </Typography>
      )}
    </Box>
  );
  if (rest.length === 0) return cell;
  // Multi-asset transaction: the collapsed `+N` expands on hover to the full
  // per-asset list, one line each, every code linking to its asset page like
  // the primary one. The tooltip surface is INVERTED (light in dark mode), so
  // these links inherit its text colour instead of the cell's accent — the
  // accent is tuned for the page background and goes unreadable on the
  // tooltip. MUI tooltips are interactive by default, so the links are
  // reachable with the pointer.
  return (
    <Tooltip
      title={
        <Stack spacing={0.25}>
          {values.map((v) => (
            <span key={v.asset}>
              {formatAmount(scaleByDecimals(v.net_settled, v.decimals), 2)}{' '}
              <Link
                component={RouterLink}
                to={routes.asset(v.asset)}
                underline="always"
                sx={{ color: 'inherit' }}
              >
                {valueCode(v)}
              </Link>
            </span>
          ))}
        </Stack>
      }
    >
      {cell}
    </Tooltip>
  );
}

/**
 * Display code for a net-settled entry: `XLM` only for native; a bespoke token
 * with no on-chain symbol has a null `asset_code` and must NOT be mislabeled
 * as XLM (its `asset` C-StrKey still links correctly).
 */
function valueCode(value: TransactionValue): string {
  return isNativeAssetString(value.asset)
    ? NATIVE_ASSET_CODE
    : value.asset_code ?? '';
}

/**
 * Ledger-sequence column, identical across the transaction/event/invocation
 * tables. Generic over the row type — any row exposing `ledger_sequence`.
 */
export function ledgerColumn<
  T extends { ledger_sequence: number }
>(): ExplorerTableColumn<T> {
  return {
    id: 'ledger',
    header: 'Ledger',
    width: 120,
    cell: (row) => (
      <IdentifierDisplay value={String(row.ledger_sequence)} type="ledger" />
    ),
  };
}

/** Transaction-hash column with copy affordance, shared by the tx tables. */
export function hashColumn<
  T extends { hash: string }
>(): ExplorerTableColumn<T> {
  return {
    id: 'hash',
    header: 'Hash',
    width: 160,
    cell: (row) => <IdentifierWithCopy value={row.hash} type="transaction" />,
  };
}

/** Success/fail status chip column, shared by the tx tables. */
export function statusColumn<
  T extends { successful: boolean }
>(): ExplorerTableColumn<T> {
  return {
    id: 'status',
    header: 'Status',
    width: 120,
    cell: (row) => <StatusChip successful={row.successful} />,
  };
}
