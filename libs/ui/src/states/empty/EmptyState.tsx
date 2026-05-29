import { Box, Stack, Typography } from '@mui/material';
import type { ElementType, ReactNode } from 'react';

import { monoFontFamily } from '../../theme/typography.js';

export type EmptyStateVariant = 'default' | 'warning' | 'error';

interface EmptyStateProps {
  icon: ReactNode;
  variant?: EmptyStateVariant;
  title: string;
  description?: ReactNode;
  action?: ReactNode;
  meta?: ReactNode;
}

export function EmptyState({
  icon,
  variant = 'default',
  title,
  description,
  action,
  meta,
}: EmptyStateProps) {
  return (
    <Stack
      alignItems="center"
      spacing={1.5}
      sx={{ p: 3, textAlign: 'center', maxWidth: 360 }}
    >
      <Box
        sx={(theme) => ({
          width: 40,
          height: 40,
          borderRadius: 8,
          display: 'inline-flex',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor:
            variant === 'warning'
              ? theme.palette.surface.warning
              : variant === 'error'
              ? theme.palette.surface.error
              : theme.palette.surface.grayMain,
          color:
            variant === 'warning'
              ? theme.palette.text.warning
              : variant === 'error'
              ? theme.palette.text.error
              : theme.palette.text.secondary,
          border:
            variant === 'default'
              ? `1px solid ${theme.palette.stroke.default}`
              : 'none',
          '& > svg': {
            width: 20,
            height: 20,
          },
        })}
      >
        {icon}
      </Box>
      <Stack alignItems="center" spacing={0.5}>
        <Typography variant="bodyLgMedium">{title}</Typography>
        {description && (
          <Typography variant="bodySmRegular" sx={{ color: 'text.secondary' }}>
            {description}
          </Typography>
        )}
        {meta && (
          <Typography
            variant="bodyXsRegular"
            sx={{
              color: 'text.tertiary',
              fontFamily: monoFontFamily,
              mt: 0.5,
            }}
          >
            {meta}
          </Typography>
        )}
      </Stack>
      {action && <Box sx={{ mt: 1 }}>{action}</Box>}
    </Stack>
  );
}
