import { Box } from '@mui/material';
import {
  DebouncedField,
  isContractId,
} from '@rumblefish/soroban-block-explorer-ui';

import { COLLECTION_COLUMN_ENABLED } from './NftsTable.js';

interface NftFiltersProps {
  collection: string;
  contractId: string;
  onCollectionChange: (value: string) => void;
  onContractIdChange: (value: string) => void;
}

/**
 * Filter bar for the NFTs list — a collection filter and a contract-ID
 * search, matching the two-input Figma design. Each commits on a debounce;
 * the page mirrors them into the URL.
 */
export function NftFilters({
  collection,
  contractId,
  onCollectionChange,
  onContractIdChange,
}: NftFiltersProps) {
  const isContractError = contractId !== '' && !isContractId(contractId);

  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        gap: 1,
        p: 2,
        flexWrap: 'wrap',
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.grayMainAlt,
      })}
    >
      {COLLECTION_COLUMN_ENABLED && (
        <DebouncedField
          value={collection}
          placeholder="Filter by collection..."
          ariaLabel="Filter by collection"
          width={400}
          onCommit={onCollectionChange}
        />
      )}
      <DebouncedField
        value={contractId}
        placeholder="Search by contract ID..."
        ariaLabel="Filter by contract ID"
        width={280}
        onCommit={onContractIdChange}
        error={isContractError}
        helperText={
          isContractError ? 'Requires a full Contract ID (C...)' : undefined
        }
      />
    </Box>
  );
}
