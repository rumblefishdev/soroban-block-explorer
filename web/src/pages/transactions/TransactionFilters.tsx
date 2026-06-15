import { Box, MenuItem, Select } from '@mui/material';
import {
  DebouncedField,
  isAccountId,
  isContractId,
} from '@rumblefish/soroban-block-explorer-ui';

import { OPERATION_TYPE_OPTIONS } from './operationTypes.js';

const ALL_OPERATIONS = '';

interface TransactionFiltersProps {
  /** Combined source-account / contract-ID search value. */
  search: string;
  /** Raw operation-type enum, or `''` for "All operation types". */
  operationType: string;
  onSearchChange: (value: string) => void;
  onOperationTypeChange: (value: string) => void;
}

/**
 * Filter bar for the Transactions list — a combined account/contract search
 * input plus an operation-type dropdown. Matches the two-control Figma design
 * (source account and contract ID share one input).
 */
export function TransactionFilters({
  search,
  operationType,
  onSearchChange,
  onOperationTypeChange,
}: TransactionFiltersProps) {
  const isError =
    search !== '' && !isAccountId(search) && !isContractId(search);

  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        gap: 2,
        p: 2,
        flexWrap: 'wrap',
        backgroundColor: theme.palette.surface.grayMainAlt,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
      })}
    >
      <DebouncedField
        value={search}
        placeholder="Source account or contract ID..."
        ariaLabel="Filter by source account or contract ID"
        width={400}
        onCommit={onSearchChange}
        error={isError}
        helperText={
          isError
            ? 'Requires a full Account (G...) or Contract ID (C...)'
            : undefined
        }
      />
      <Select
        value={operationType}
        onChange={(e) => onOperationTypeChange(e.target.value)}
        displayEmpty
        aria-label="Filter by operation type"
        sx={{ width: { xs: '100%', sm: 280 } }}
      >
        <MenuItem value={ALL_OPERATIONS}>All operation types</MenuItem>
        {OPERATION_TYPE_OPTIONS.map((option) => (
          <MenuItem key={option.value} value={option.value}>
            {option.label}
          </MenuItem>
        ))}
      </Select>
    </Box>
  );
}
