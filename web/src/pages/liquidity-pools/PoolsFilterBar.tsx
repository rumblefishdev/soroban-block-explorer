import { Box, MenuItem, Select, type SelectChangeEvent } from '@mui/material';
import { DebouncedField } from '@rumblefish/soroban-block-explorer-ui';

/**
 * TVL preset options (Figma node 267:60674).
 *
 * Empty value (`""`) maps to "Any TVL" — no `filter[min_tvl]` sent.
 * Other values are the raw decimal thresholds the API expects.
 */
/**
 * Hides the TVL preset filter until the backend exposes a TVL column /
 * value to filter against (no TVL is shown anywhere in the UI, so the
 * filter reads as broken). Build-time `const false` — same pattern as
 * `CHARTS_ENABLED` in pool-detail (task 0341). Flip to `true` once TVL
 * lands. Task 0351 (F10).
 */
const TVL_FILTER_ENABLED = false;

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
        // Says what the filter actually does: a fragment matches either leg,
        // a pair constrains both. The old "Filter by asset pair..." promised
        // a syntax the backend rejected (task 0440).
        placeholder="Filter by asset — USDC, or a pair USDC/XLM"
        ariaLabel="Filter by asset or asset pair"
        width={400}
        onCommit={onAssetChange}
      />
      {TVL_FILTER_ENABLED && (
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
      )}
    </Box>
  );
}
