import KeyboardArrowRightIcon from '@mui/icons-material/KeyboardArrowRight';
import { Box, Typography } from '@mui/material';
import type { SxProps, Theme } from '@mui/material';
import type { ReactNode } from 'react';

interface DisclosureRowProps {
  open: boolean;
  onToggle: () => void;
  label: ReactNode;
  /** Chips/counters rendered after the label. */
  trailing?: ReactNode;
  sx?: SxProps<Theme>;
}

/** The page's one disclosure header: chevron + label, keyboard-operable,
 *  `aria-expanded` announced. Pair with `<Collapse>` in the caller. */
export function DisclosureRow({
  open,
  onToggle,
  label,
  trailing,
  sx,
}: DisclosureRowProps) {
  return (
    <Box
      role="button"
      tabIndex={0}
      aria-expanded={open}
      onClick={onToggle}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onToggle();
        }
      }}
      sx={[
        (theme) => ({
          display: 'flex',
          alignItems: 'center',
          gap: 0.75,
          cursor: 'pointer',
          color: theme.palette.text.secondary,
        }),
        ...(Array.isArray(sx) ? sx : [sx]),
      ]}
    >
      <KeyboardArrowRightIcon
        sx={{
          fontSize: 18,
          transform: open ? 'rotate(90deg)' : 'none',
          transition: 'transform 120ms ease',
        }}
      />
      <Typography variant="bodySmSemiBold" sx={{ color: 'inherit' }}>
        {label}
      </Typography>
      {trailing}
    </Box>
  );
}
