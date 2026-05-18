import { Typography } from '@mui/material';
import type { NftItem } from '@rumblefish/api-types';
import {
  ExplorerTable,
  IdentifierDisplay,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import { NftNameCell } from './NftNameCell.js';

interface NftsTableProps {
  rows: readonly NftItem[];
}

function Dash() {
  return (
    <Typography component="span" sx={{ color: 'text.tertiary' }}>
      —
    </Typography>
  );
}

const columns: ExplorerTableColumn<NftItem>[] = [
  {
    id: 'nft',
    header: 'NFT',
    cell: (row) => <NftNameCell row={row} />,
  },
  {
    id: 'collection',
    header: 'Collection',
    cell: (row) =>
      row.collection_name ? (
        <Typography component="span" variant="bodySmRegular">
          {row.collection_name}
        </Typography>
      ) : (
        <Dash />
      ),
  },
  {
    id: 'contract',
    header: 'Contract ID',
    cell: (row) => (
      <IdentifierDisplay value={row.contract_id} type="contract" />
    ),
  },
  {
    id: 'owner',
    header: 'Owner',
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
export function NftsTable({ rows }: NftsTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => String(row.id)}
    />
  );
}
