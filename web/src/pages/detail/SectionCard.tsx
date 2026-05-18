import { Box, Card, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

interface SectionCardProps {
  /** Section heading, e.g. "Summary", "Balances". */
  title: string;
  /** Optional secondary text next to the title, e.g. "4 assets". */
  meta?: ReactNode;
  /** Optional element pinned to the right of the header. */
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
    <Card>
      <Box
        sx={(theme) => ({
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 2,
          p: 2,
          borderBottom: `1px solid ${theme.palette.stroke.default}`,
        })}
      >
        <Stack direction="row" spacing={1} alignItems="baseline">
          <Typography variant="heading5SemiBold" component="h2">
            {title}
          </Typography>
          {meta != null && (
            <Typography
              variant="bodySmRegular"
              sx={{ color: 'text.secondary' }}
            >
              {meta}
            </Typography>
          )}
        </Stack>
        {action}
      </Box>
      {children}
    </Card>
  );
}
