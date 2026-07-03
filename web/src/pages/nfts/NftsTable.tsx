import { Typography } from '@mui/material';
import type { NftItem } from '@rumblefish/api-types';
import {
  Dash,
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  IdentifierDisplay,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import { NftNameCell } from './NftNameCell.js';

interface NftsTableProps {
  rows: readonly NftItem[];
  loading?: boolean;
  skeletonRows?: number;
}

/**
 * Hides the Collection column until the backend populates
 * `collection_name` (currently null for every NFT, so the column is `—`
 * on every row and reads as broken). Build-time `const false` — same
 * pattern as `TVL_FILTER_ENABLED` / `CHARTS_ENABLED` (tasks 0351/0341).
 * Flip to `true` once collections land. Task 0351 (F8). Also gates the
 * "Filter by collection" input in `NftFilters` — the filter targets the
 * same always-null field, so it must hide/show together with the column.
 */
export const COLLECTION_COLUMN_ENABLED = false;

const columns: ExplorerTableColumn<NftItem>[] = [
  {
    id: 'nft',
    header: 'NFT',
    width: 240,
    cell: (row) => <NftNameCell row={row} />,
  },
  ...(COLLECTION_COLUMN_ENABLED
    ? [
        {
          id: 'collection',
          header: 'Collection',
          width: 180,
          cell: (row: NftItem) =>
            row.collection_name ? (
              <Typography
                component="span"
                variant="bodySmMedium"
                sx={(theme) => ({ color: theme.palette.text.primary })}
              >
                {row.collection_name}
              </Typography>
            ) : (
              <Dash />
            ),
        },
      ]
    : []),
  {
    id: 'contract',
    header: 'Contract ID',
    width: 160,
    cell: (row) => (
      <IdentifierDisplay value={row.contract_id} type="contract" />
    ),
  },
  {
    id: 'owner',
    header: 'Owner',
    width: 160,
    // `owner_account` is null for burned NFTs (ADR 0037 §13).
    cell: (row) =>
      row.owner_account ? (
        <IdentifierDisplay value={row.owner_account} type="account" />
      ) : (
        <Dash />
      ),
  },
];

/** Column count — used to size the loading skeleton consistently. */
export const NFT_COLUMN_COUNT = columns.length;

/**
 * The NFTs list table — preview + name (linked to detail), collection,
 * contract id and current owner, per the Figma design.
 */
export function NftsTable({ rows, loading, skeletonRows }: NftsTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => `${row.contract_id}:${row.token_id}`}
      rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
      loading={loading}
      skeletonRows={skeletonRows}
    />
  );
}
