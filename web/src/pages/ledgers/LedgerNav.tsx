import ArrowBackIcon from '@mui/icons-material/ArrowBackRounded';
import ArrowForwardIcon from '@mui/icons-material/ArrowForwardRounded';
import { Box, Button } from '@mui/material';
import type { Theme } from '@mui/material/styles';
import type { ReactNode } from 'react';
import { Link as RouterLink } from 'react-router-dom';

import { routes } from '../../router/routes.js';

interface LedgerNavProps {
  prevSequence?: number | null;
  nextSequence?: number | null;
}

const pillSx = (theme: Theme) => ({
  borderRadius: `${theme.shape.radius.pills}px`,
  backgroundColor: theme.palette.surface.grayMain,
  border: `1px solid ${theme.palette.stroke.default}`,
  color: theme.palette.text.primary,
  textTransform: 'none' as const,
  px: 1.5,
  py: 0.75,
  gap: 1,
  ...theme.typography.bodySmMedium,
  '&:hover': { backgroundColor: theme.palette.surface.grayHover },
  '&.Mui-disabled': {
    opacity: 0.5,
    color: theme.palette.text.tertiary,
  },
});

function YellowCircle({ children }: { children: ReactNode }) {
  return (
    <Box
      component="span"
      sx={(theme) => ({
        width: 20,
        height: 20,
        borderRadius: '50%',
        flexShrink: 0,
        backgroundColor: theme.palette.surface.primaryMain,
        color: theme.palette.common.black,
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        '& svg': { fontSize: 14 },
      })}
    >
      {children}
    </Box>
  );
}

interface NavPillProps {
  to?: string;
  label: string;
  icon: ReactNode;
  iconSide: 'start' | 'end';
}

function NavPill({ to, label, icon, iconSide }: NavPillProps) {
  const circle = <YellowCircle>{icon}</YellowCircle>;
  const content = (
    <>
      {iconSide === 'start' && circle}
      <Box component="span">{label}</Box>
      {iconSide === 'end' && circle}
    </>
  );
  if (!to) {
    return (
      <Button disabled sx={pillSx}>
        {content}
      </Button>
    );
  }
  return (
    <Button component={RouterLink} to={to} sx={pillSx}>
      {content}
    </Button>
  );
}

/**
 * Previous / next ledger navigation. Targets come from the API's
 * `prev_sequence` / `next_sequence` (gap-aware); a null neighbour
 * renders a disabled pill.
 */
export function LedgerNav({ prevSequence, nextSequence }: LedgerNavProps) {
  return (
    <Box sx={{ display: 'flex', gap: 1.5, flexShrink: 0 }}>
      <NavPill
        to={prevSequence != null ? routes.ledger(prevSequence) : undefined}
        label="Prev ledger"
        icon={<ArrowBackIcon />}
        iconSide="start"
      />
      <NavPill
        to={nextSequence != null ? routes.ledger(nextSequence) : undefined}
        label="Next ledger"
        icon={<ArrowForwardIcon />}
        iconSide="end"
      />
    </Box>
  );
}
