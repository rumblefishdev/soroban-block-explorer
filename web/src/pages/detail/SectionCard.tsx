import { Box, Card, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

interface SectionCardProps {
  title: ReactNode;
  meta?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
}

/**
 * A card with a titled header, used for the composed sections of the detail
 * pages (account summary, balances, asset metadata, transaction lists).
 */
export function SectionCard({
  title,
  meta,
  action,
  children,
}: SectionCardProps) {
  return (
    <Card
      sx={(theme) => ({
        backgroundColor: theme.palette.surface.grayMainAlt,
      })}
    >
      <Box
        sx={(theme) => ({
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 2,
          p: 2,

          // Card header sits on the darker surface; body stays on the
          // lighter Card surface (Figma "Table sections" vs "Slot").
          backgroundColor: theme.palette.surface.grayMainAlt,
          borderBottom: `1px solid ${theme.palette.stroke.default}`,
        })}
      >
        <Stack spacing={0.25}>
          {typeof title === 'string' ? (
            <Typography variant="heading5SemiBold" component="h2">
              {title}
            </Typography>
          ) : (
            title
          )}
          {meta != null && (
            <Typography
              variant="bodyMedium"
              sx={(theme) => ({ color: theme.palette.text.secondary })}
            >
              {meta}
            </Typography>
          )}
        </Stack>
        {action}
      </Box>
      <Box
        sx={(theme) => ({
          backgroundColor: theme.palette.surface.grayMain,
        })}
      >
        {children}
      </Box>
    </Card>
  );
}
