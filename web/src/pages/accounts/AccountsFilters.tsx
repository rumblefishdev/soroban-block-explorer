import { Box, Divider, MenuItem, Select, Stack } from '@mui/material';
import { Chip, DebouncedField } from '@rumblefish/soroban-block-explorer-ui';

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
        placeholder="Search by account address..."
        ariaLabel="Search by account address"
        width={360}
        onCommit={onSearchChange}
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
