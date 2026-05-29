import SearchIcon from '@mui/icons-material/SearchOutlined';
import {
  Box,
  InputAdornment,
  MenuItem,
  Select,
  TextField,
  type SelectChangeEvent,
} from '@mui/material';
import { useEffect, useState } from 'react';

const SEARCH_DEBOUNCE_MS = 300;

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
  const [draft, setDraft] = useState(asset);

  // Keep the local draft in sync if the URL value changes externally
  // (e.g. browser back/forward, programmatic reset).
  useEffect(() => {
    setDraft(asset);
  }, [asset]);

  // Debounce keystrokes; commits to the URL after the user pauses typing.
  useEffect(() => {
    if (draft === asset) return;
    const id = setTimeout(() => onAssetChange(draft), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(id);
  }, [draft, asset, onAssetChange]);

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
      <TextField
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        placeholder="Filter by asset pair..."
        aria-label="Filter by asset pair"
        sx={{ width: 400, maxWidth: '100%' }}
        slotProps={{
          input: {
            startAdornment: (
              <InputAdornment position="start">
                <SearchIcon
                  sx={(theme) => ({
                    fontSize: 18,
                    color: theme.palette.text.tertiary,
                  })}
                />
              </InputAdornment>
            ),
          },
        }}
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
