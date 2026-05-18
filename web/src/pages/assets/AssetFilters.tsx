import SearchIcon from '@mui/icons-material/SearchOutlined';
import { Box, InputAdornment, Stack, TextField } from '@mui/material';
import { Chip } from '@rumblefish/soroban-block-explorer-ui';
import { useEffect, useState } from 'react';

import { ASSET_TYPE_FILTERS } from './assetType.js';

const SEARCH_DEBOUNCE_MS = 300;

interface AssetFiltersProps {
  /** Asset-code search value (`filter[code]`). */
  search: string;
  /** Active asset-type filter, or `''` for "All types". */
  type: string;
  onSearchChange: (value: string) => void;
  onTypeChange: (value: string) => void;
}

/**
 * Filter bar for the assets list — an asset-code search input plus a row of
 * type chips (All types / Classic / SAC / Soroban), matching the Figma design.
 */
export function AssetFilters({
  search,
  type,
  onSearchChange,
  onTypeChange,
}: AssetFiltersProps) {
  const [draft, setDraft] = useState(search);

  useEffect(() => {
    setDraft(search);
  }, [search]);

  useEffect(() => {
    if (draft === search) return;
    const id = setTimeout(() => onSearchChange(draft), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(id);
  }, [draft, search, onSearchChange]);

  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        flexWrap: 'wrap',
        gap: 2,
        p: 2,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
      })}
    >
      <TextField
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        placeholder="Search by asset code..."
        aria-label="Search by asset code"
        sx={{ flex: 1, minWidth: 240 }}
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
      <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
        {ASSET_TYPE_FILTERS.map((option) => {
          const active = option.value === type;
          return (
            <Chip
              key={option.value}
              label={option.label}
              size="md"
              color={active ? 'accent' : 'neutral'}
              clickable
              onClick={() => onTypeChange(option.value)}
              aria-pressed={active}
            />
          );
        })}
      </Stack>
    </Box>
  );
}
