import SearchIcon from '@mui/icons-material/Search';
import { Box, Typography } from '@mui/material';
import { alpha } from '@mui/material/styles';
import {
  monoFontFamily,
  searchShortcutLabel,
} from '@rumblefish/soroban-block-explorer-ui';

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
        borderRadius: `${theme.shape.radius.s}px`,
        border: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.grayMain,
        transition: 'border-color 0.15s ease, box-shadow 0.15s ease',
        // Border lightens on hover and takes `stroke.action` on focus, same
        // token as the header search. The brand-yellow focus halo on top is
        // hero-only and deliberate: this is the landing page's primary call
        // to action, the header field is a compact utility. The halo needs
        // more presence against a light surface than a dark one.
        '&:hover': {
          borderColor: theme.palette.stroke.defaultHover,
        },
        '&:focus-within': {
          borderColor: theme.palette.stroke.action,
          boxShadow: `0 0 0 3px ${alpha(
            theme.palette.surface.primaryMain,
            theme.palette.mode === 'light' ? 0.34 : 0.22
          )}`,
        },
      })}
    >
      <SearchIcon
        sx={(theme) => ({
          width: 16,
          height: 16,
          flexShrink: 0,
          color: theme.palette.text.secondary,
        })}
      />
      <Box
        component="input"
        data-global-search="true"
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
          borderRadius: `${theme.shape.radius.s}px`,
          backgroundColor: theme.palette.surface.grayLight,
        })}
      >
        <Typography
          variant="bodyXsMedium"
          color="text.tertiary"
          noWrap
          sx={{ fontFamily: monoFontFamily }}
        >
          {searchShortcutLabel}
        </Typography>
      </Box>
    </Box>
  );
}
