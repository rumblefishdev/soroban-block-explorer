import { Box, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

export interface TableSectionHeaderProps {
  title: ReactNode;
  description?: ReactNode;
  badge?: ReactNode;
  action?: ReactNode;
}

export function TableSectionHeader({
  title,
  description,
  badge,
  action,
}: TableSectionHeaderProps) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 2,
        p: 2,
        backgroundColor: theme.palette.surface.grayMainAlt,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
      })}
    >
      <Stack spacing={1} alignItems="flex-start">
        <Stack direction="row" spacing={1} alignItems="center">
          <Typography variant="heading5SemiBold" color="text.primary">
            {title}
          </Typography>
          {badge}
        </Stack>
        {description ? (
          <Typography variant="bodyMedium" color="text.secondary">
            {description}
          </Typography>
        ) : null}
      </Stack>
      {action ? <Box sx={{ flexShrink: 0 }}>{action}</Box> : null}
    </Box>
  );
}
