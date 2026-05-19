import SearchIcon from '@mui/icons-material/Search';
import { Box, Typography } from '@mui/material';
import { monoFontFamily } from '@rumblefish/soroban-block-explorer-ui';

interface HeroSearchProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit?: () => void;
  placeholder?: string;
}

/**
 * Home hero search — a static, full-width search box matching the Figma
 * hero "Search input" (node 3:2333): 56px tall, 8px radius, magnifier +
 * placeholder + a "CTRL + K" hint pill. Distinct from the header
 * `SearchInput`, which collapses and expands on hover/focus.
 */
export function HeroSearch({
  value,
  onChange,
  onSubmit,
  placeholder = 'Search by TX hash, accounts, contract, token',
}: HeroSearchProps) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'center',
        gap: 1,
        width: '100%',
        height: 56,
        px: 2,
        boxSizing: 'border-box',
        borderRadius: '8px',
        border: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.grayMain,
        transition: 'border-color 0.15s ease',
        // Lighten the border on hover, accent it on focus — same
        // affordance as the header search.
        '&:hover': {
          borderColor: theme.palette.stroke.defaultHover,
        },
        '&:focus-within': {
          borderColor: theme.palette.stroke.action,
        },
      })}
    >
      <SearchIcon
        sx={{ width: 16, height: 16, flexShrink: 0, color: 'text.secondary' }}
      />
      <Box
        component="input"
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') onSubmit?.();
        }}
        placeholder={placeholder}
        aria-label={placeholder}
        sx={(theme) => ({
          flex: 1,
          minWidth: 0,
          border: 'none',
          outline: 'none',
          background: 'transparent',
          fontFamily: theme.typography.bodyMedium.fontFamily,
          fontSize: theme.typography.bodyMedium.fontSize,
          fontWeight: theme.typography.bodyMedium.fontWeight,
          lineHeight: theme.typography.bodyMedium.lineHeight,
          letterSpacing: theme.typography.bodyMedium.letterSpacing,
          color: theme.palette.text.primary,
          textOverflow: 'ellipsis',
          '&::placeholder': { color: theme.palette.text.secondary },
        })}
      />
      <Box
        sx={(theme) => ({
          display: 'inline-flex',
          alignItems: 'center',
          justifyContent: 'center',
          height: 20,
          px: '6px',
          flexShrink: 0,
          borderRadius: '8px',
          backgroundColor: theme.palette.surface.grayLight,
        })}
      >
        <Typography
          variant="bodyXsMedium"
          color="text.tertiary"
          noWrap
          sx={{ fontFamily: monoFontFamily }}
        >
          CTRL + K
        </Typography>
      </Box>
    </Box>
  );
}
