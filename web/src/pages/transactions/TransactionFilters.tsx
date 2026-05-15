import SearchIcon from '@mui/icons-material/SearchOutlined';
import {
  Box,
  InputAdornment,
  MenuItem,
  Select,
  TextField,
} from '@mui/material';
import { useEffect, useState } from 'react';

import { OPERATION_TYPE_OPTIONS } from './operationTypes.js';

const ALL_OPERATIONS = '';
const SEARCH_DEBOUNCE_MS = 300;

interface TransactionFiltersProps {
  /** Combined source-account / contract-ID search value. */
  search: string;
  /** Raw operation-type enum, or `''` for "All operations type". */
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
  const [draft, setDraft] = useState(search);

  // Re-sync the local input when the value changes externally
  // (e.g. the "Clear filters" action).
  useEffect(() => {
    setDraft(search);
  }, [search]);

  // Debounce committing the typed value so we don't refetch per keystroke.
  useEffect(() => {
    if (draft === search) return;
    const id = setTimeout(() => onSearchChange(draft), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(id);
  }, [draft, search, onSearchChange]);

  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        gap: 2,
        p: 2,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
      })}
    >
      <TextField
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        placeholder="Source account or contract ID..."
        aria-label="Filter by source account or contract ID"
        fullWidth
        sx={{ flex: 1 }}
        slotProps={{
          input: {
            startAdornment: (
              <InputAdornment position="start">
                <SearchIcon sx={{ fontSize: 18, color: 'text.tertiary' }} />
              </InputAdornment>
            ),
          },
        }}
      />
      <Select
        value={operationType}
        onChange={(e) => onOperationTypeChange(e.target.value)}
        displayEmpty
        aria-label="Filter by operation type"
        sx={{ minWidth: 240 }}
      >
        <MenuItem value={ALL_OPERATIONS}>All operations type</MenuItem>
        {OPERATION_TYPE_OPTIONS.map((option) => (
          <MenuItem key={option.value} value={option.value}>
            {option.label}
          </MenuItem>
        ))}
      </Select>
    </Box>
  );
}
