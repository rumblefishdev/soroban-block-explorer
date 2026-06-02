import SearchIcon from '@mui/icons-material/SearchOutlined';
import {
  Box,
  Divider,
  InputAdornment,
  MenuItem,
  Select,
  Stack,
  TextField,
} from '@mui/material';
import { Chip, useDebouncedDraft } from '@rumblefish/soroban-block-explorer-ui';

import type { AccountsSort } from '../../api/hooks/useAccountsList.js';

const SORT_OPTIONS: { value: AccountsSort; label: string }[] = [
  { value: 'xlm_desc', label: 'Top XLM holders' },
  { value: 'last_seen_desc', label: 'Recently active' },
  { value: 'first_seen_desc', label: 'New accounts' },
];

interface AccountsFiltersProps {
  search: string;
  sort: AccountsSort;
  withDomain: boolean;
  onSearchChange: (value: string) => void;
  onSortChange: (value: AccountsSort) => void;
  onWithDomainChange: (value: boolean) => void;
}

export function AccountsFilters({
  search,
  sort,
  withDomain,
  onSearchChange,
  onSortChange,
  onWithDomainChange,
}: AccountsFiltersProps) {
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
        placeholder="Search by account address..."
        aria-label="Search by account address"
        sx={{ width: { xs: '100%', sm: 360 } }}
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
      <Select
        value={sort}
        onChange={(e) => onSortChange(e.target.value as AccountsSort)}
        aria-label="Sort accounts"
        sx={{ width: { xs: '100%', sm: 220 } }}
      >
        {SORT_OPTIONS.map((option) => (
          <MenuItem key={option.value} value={option.value}>
            {option.label}
          </MenuItem>
        ))}
      </Select>
      <Stack direction="row" spacing={1}>
        <Chip
          label="With domain"
          size="lg"
          color={withDomain ? 'accent' : 'neutral'}
          clickable
          onClick={() => onWithDomainChange(!withDomain)}
          aria-pressed={withDomain}
        />
      </Stack>
    </Box>
  );
}
