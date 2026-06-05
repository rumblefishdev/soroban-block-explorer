import { Box, MenuItem, Select, type SelectChangeEvent } from '@mui/material';
import { DebouncedField } from '@rumblefish/soroban-block-explorer-ui';

/**
 * TVL preset options (Figma node 267:60674).
 *
 * Empty value (`""`) maps to "Any TVL" — no `filter[min_tvl]` sent.
 * Other values are the raw decimal thresholds the API expects.
 */
export const TVL_PRESETS: ReadonlyArray<{ value: string; label: string }> = [
  { value: '', label: 'Any TVL' },
  { value: '10000', label: 'Min $10,000' },
  { value: '100000', label: 'Min $100,000' },
  { value: '1000000', label: 'Min $1,000,000' },
];

interface PoolsFilterBarProps {
  /** Asset-code search value (URL key `asset`, API `filter[asset_code]`). */
  asset: string;
  /** Active TVL preset (URL key `min_tvl`, API `filter[min_tvl]`). */
  minTvl: string;
  onAssetChange: (value: string) => void;
  onMinTvlChange: (value: string) => void;
}

/**
 * Filter bar for the liquidity-pools list — text input for asset-code
 * search plus a TVL preset dropdown. Geometry and surface tokens match
 * Figma node `266:36052` (search 400px, TVL 280px, alt-gray surface
 * with bottom divider).
 */
export function PoolsFilterBar({
  asset,
  minTvl,
  onAssetChange,
  onMinTvlChange,
}: PoolsFilterBarProps) {
  const handleTvlChange = (event: SelectChangeEvent<string>) => {
    onMinTvlChange(event.target.value);
  };

  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        flexWrap: 'wrap',
        gap: 1,
        p: 2,

        backgroundColor: theme.palette.surface.grayMainAlt,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
      })}
    >
      <DebouncedField
        value={asset}
        placeholder="Filter by asset pair..."
        ariaLabel="Filter by asset pair"
        width={400}
        onCommit={onAssetChange}
      />
      <Select
        value={minTvl}
        onChange={handleTvlChange}
        aria-label="Minimum TVL"
        size="small"
        // `displayEmpty` keeps the "Any TVL" label visible when no
        // preset is active — otherwise MUI renders an empty box and
        // the dropdown's purpose is invisible.
        displayEmpty
        renderValue={(value) =>
          TVL_PRESETS.find((opt) => opt.value === value)?.label ?? 'Any TVL'
        }
        sx={{ width: 280, maxWidth: '100%' }}
      >
        {TVL_PRESETS.map((option) => (
          <MenuItem key={option.value || 'any'} value={option.value}>
            {option.label}
          </MenuItem>
        ))}
      </Select>
    </Box>
  );
}
