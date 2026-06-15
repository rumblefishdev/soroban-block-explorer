import { Box, Stack } from '@mui/material';
import { Chip } from '@rumblefish/soroban-block-explorer-ui';

interface AccountsFiltersProps {
  /** `filter[with_domain]` toggle. */
  withDomain: boolean;
  onWithDomainChange: (value: boolean) => void;
}

/**
 * Filter bar for the accounts list — a single "With domain" toggle (keep only
 * known/anchor accounts). No address search: account StrKeys are opaque, so a
 * prefix filter is useless and exact lookup is the global search's job.
 * Sorting lives on the "Last Seen" column header.
 */
export function AccountsFilters({
  withDomain,
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
