import SearchIcon from '@mui/icons-material/SearchOutlined';
import { Box, InputAdornment, TextField } from '@mui/material';
import { useDebouncedDraft } from '@rumblefish/soroban-block-explorer-ui';

const SEARCH_DEBOUNCE_MS = 300;

interface DebouncedFieldProps {
  value: string;
  placeholder: string;
  ariaLabel: string;
  /** Fixed input width in pixels (per the Figma filter bar). */
  width: number;
  onCommit: (value: string) => void;
}

/**
 * A search input that commits its value after a short pause, so filtering
 * does not refetch on every keystroke. Re-syncs when the value changes
 * externally (e.g. a "Clear filters" action).
 */
function DebouncedField({
  value,
  placeholder,
  ariaLabel,
  width,
  onCommit,
}: DebouncedFieldProps) {
  const [draft, setDraft] = useDebouncedDraft(
    value,
    onCommit,
    SEARCH_DEBOUNCE_MS
  );

  return (
    <TextField
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      placeholder={placeholder}
      aria-label={ariaLabel}
      sx={{ width: { xs: '100%', sm: width }, maxWidth: '100%' }}
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
  );
}

interface NftFiltersProps {
  collection: string;
  contractId: string;
  onCollectionChange: (value: string) => void;
  onContractIdChange: (value: string) => void;
}

/**
 * Filter bar for the NFTs list — a collection filter and a contract-ID
 * search, matching the two-input Figma design. Each commits on a debounce;
 * the page mirrors them into the URL.
 */
export function NftFilters({
  collection,
  contractId,
  onCollectionChange,
  onContractIdChange,
}: NftFiltersProps) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        gap: 1,
        p: 2,
        flexWrap: 'wrap',
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.grayMainAlt,
      })}
    >
      <DebouncedField
        value={collection}
        placeholder="Filter by collection..."
        ariaLabel="Filter by collection"
        width={400}
        onCommit={onCollectionChange}
      />
      <DebouncedField
        value={contractId}
        placeholder="Search by contract ID..."
        ariaLabel="Filter by contract ID"
        width={280}
        onCommit={onContractIdChange}
      />
    </Box>
  );
}
