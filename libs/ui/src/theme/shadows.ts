import type { Shadows } from '@mui/material/styles';

export const shadows = {
  xs: '0px 2px 4px 0px rgba(10, 13, 18, 0.05)',
  sm: '0px 4px 8px 0px rgba(10, 13, 18, 0.10)',
  md: '0px 8px 16px -2px rgba(10, 13, 18, 0.10)',
  lg: '0px 16px 24px 0px rgba(10, 13, 18, 0.12)',
  xl: '0px 24px 48px 0px rgba(10, 13, 18, 0.10)',
  '2xl': '0px 32px 64px 0px rgba(10, 13, 18, 0.18)',
} as const;

export const muiShadows: Shadows = [
  'none',
  shadows.xs,
  shadows.xs,
  shadows.sm,
  shadows.sm,
  shadows.md,
  shadows.md,
  shadows.md,
  shadows.lg,
  shadows.lg,
  shadows.lg,
  shadows.lg,
  shadows.lg,
  shadows.xl,
  shadows.xl,
  shadows.xl,
  shadows.xl,
  shadows.xl,
  shadows.xl,
  shadows.xl,
  shadows.xl,
  shadows.xl,
  shadows.xl,
  shadows.xl,
  shadows['2xl'],
];
