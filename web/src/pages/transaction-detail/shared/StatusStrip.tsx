import ErrorOutlineIcon from '@mui/icons-material/ErrorOutline';
import WarningAmberOutlinedIcon from '@mui/icons-material/WarningAmberOutlined';
import { Box, Typography } from '@mui/material';
import type { ReactNode } from 'react';

/**
 * A full-bleed status line across the top of a section card.
 *
 * Deliberately NOT an MUI `Alert`: the theme carries no `MuiAlert` style, so an
 * outlined Alert derives its border from `warning.light` — a FILL token here —
 * and the border disappears against the card in light mode (0460 #8). The strip
 * borrows the matching Chip palette instead and stays in the house style.
 */
export function StatusStrip({
  tone,
  children,
}: {
  tone: 'error' | 'warning';
  children: ReactNode;
}) {
  const Icon = tone === 'error' ? ErrorOutlineIcon : WarningAmberOutlinedIcon;
  return (
    <Box
      role="status"
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'center',
        gap: 1,
        px: 2,
        py: 0.75,
        backgroundColor: theme.palette.surface[tone],
        borderBottom: `1px solid ${theme.palette.stroke[tone]}`,
      })}
    >
      <Icon
        sx={(theme) => ({ fontSize: 16, color: theme.palette.text[tone] })}
      />
      <Typography
        variant="bodySmRegular"
        sx={(theme) => ({ color: theme.palette.text[tone] })}
      >
        {children}
      </Typography>
    </Box>
  );
}
