import SearchIcon from '@mui/icons-material/SearchOutlined';
import { Box, Divider, InputAdornment, Stack, TextField } from '@mui/material';
import { Chip, useDebouncedDraft } from '@rumblefish/soroban-block-explorer-ui';

import { CONTRACT_TYPE_FILTERS } from './contractType.js';

interface ContractsFiltersProps {
  /** Search value (`filter[q]`) — contract id or name. */
  search: string;
  /** Active contract-type filter, or `''` for "All types". */
  type: string;
  onSearchChange: (value: string) => void;
  onTypeChange: (value: string) => void;
}

/**
 * Filter bar for the contracts list — a contract id/name search input plus a
 * row of type chips (All / SAC / NFT / Fungible / Other). Mirrors the
 * assets filter bar.
 */
export function ContractsFilters({
  search,
  type,
  onSearchChange,
  onTypeChange,
}: ContractsFiltersProps) {
  const [draft, setDraft] = useDebouncedDraft(search, onSearchChange);

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
      <TextField
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        placeholder="Search by contract id or name..."
        aria-label="Search by contract id or name"
        sx={{ width: { xs: '100%', sm: 320 } }}
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
      <Divider
        orientation="vertical"
        flexItem
        sx={{ display: { xs: 'none', sm: 'block' }, my: 0.5 }}
      />
      <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
        {CONTRACT_TYPE_FILTERS.map((option) => {
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
