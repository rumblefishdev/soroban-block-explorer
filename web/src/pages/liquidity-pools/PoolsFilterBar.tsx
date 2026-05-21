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
 * search plus a TVL preset dropdown. Matches Figma list page.
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
        gap: 2,
        p: 2,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
      })}
    >
      <TextField
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        placeholder="Filter by asset pair"
        aria-label="Filter by asset pair"
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
      <Select
        value={minTvl}
        onChange={handleTvlChange}
        aria-label="Minimum TVL"
        size="small"
        sx={{ minWidth: 180 }}
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
