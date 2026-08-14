import { Box, Divider, Stack } from '@mui/material';
import { Chip, DebouncedField } from '@rumblefish/soroban-block-explorer-ui';

import { ASSET_TYPE_FILTERS } from './assetType.js';

interface AssetFiltersProps {
  /** Asset-code search value (`filter[code]`). */
  search: string;
  /** Active asset-type filter, or `''` for "All types". */
  type: string;
  /** Whether the "Has SAC" property filter is active (`filter[sac]=true`). */
  sac: boolean;
  onSearchChange: (value: string) => void;
  onTypeChange: (value: string) => void;
  onSacChange: (value: boolean) => void;
}

/**
 * Filter bar for the assets list — an asset-code search input, a row of type
 * chips (All types / Classic credit / Soroban), and a separate "Has SAC" property
 * toggle. Type and the SAC facet are orthogonal axes (ADR 0051): an asset has
 * a type AND may additionally carry a deployed SAC, so SAC is a property toggle,
 * not a type chip.
 */
export function AssetFilters({
  search,
  type,
  sac,
  onSearchChange,
  onTypeChange,
  onSacChange,
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
      <Divider
        orientation="vertical"
        flexItem
        sx={{ display: { xs: 'none', sm: 'block' }, my: 0.5 }}
      />
      <Chip
        label="Has SAC"
        size="lg"
        color={sac ? 'accent' : 'neutral'}
        clickable
        onClick={() => onSacChange(!sac)}
        aria-pressed={sac}
      />
    </Box>
  );
}
