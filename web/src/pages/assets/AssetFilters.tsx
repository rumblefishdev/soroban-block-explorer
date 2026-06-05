import { Box, Divider, Stack } from '@mui/material';
import { Chip, DebouncedField } from '@rumblefish/soroban-block-explorer-ui';

import { ASSET_TYPE_FILTERS } from './assetType.js';

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
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        flexWrap: 'wrap',
        alignItems: 'center',
        gap: 2,
        p: 2,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        bgcolor: theme.palette.surface.grayMainAlt,
      })}
    >
      <DebouncedField
        value={search}
        placeholder="Search by asset code..."
        ariaLabel="Search by asset code"
        width={320}
        onCommit={onSearchChange}
      />
      <Divider
        orientation="vertical"
        flexItem
        sx={{ display: { xs: 'none', sm: 'block' }, my: 0.5 }}
      />
      <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
        {ASSET_TYPE_FILTERS.map((option) => {
          const active = option.value === type;
          return (
            <Chip
              key={option.value}
              label={option.label}
              size="lg"
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
