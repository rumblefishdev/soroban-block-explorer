import { Box, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

interface PageHeaderProps {
  title: ReactNode;
  subtitle?: ReactNode;
  action?: ReactNode;
}

export function PageHeader({ title, subtitle, action }: PageHeaderProps) {
  return (
    <Box
      sx={{
        display: 'flex',
        alignItems: action ? 'flex-end' : 'flex-start',
        justifyContent: 'space-between',
        gap: 2,
        flexWrap: 'wrap',
      }}
    >
      <Stack spacing={1}>
        <Typography variant="heading5SemiBold" component="h1">
          {title}
        </Typography>
        {subtitle != null && (
          <Typography
            variant="bodyMedium"
            sx={(theme) => ({ color: theme.palette.text.secondary })}
          >
            {subtitle}
          </Typography>
        )}
      </Stack>
      {action != null && <Box sx={{ flexShrink: 0 }}>{action}</Box>}
    </Box>
  );
}
