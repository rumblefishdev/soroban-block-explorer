import { Box, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

export interface SummaryCell {
  label: string;
  value: ReactNode;
}

/**
 * One row of a summary card. Pass a single cell for a full-width row, or two
 * cells for a side-by-side row (e.g. "First seen ledger" / "Last seen ledger").
 */
export function SummaryRow({ cells }: { cells: SummaryCell[] }) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        flexDirection: { xs: 'column', sm: 'row' },
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        '&:last-of-type': { borderBottom: 'none' },
      })}
    >
      {cells.map((cell, index) => (
        <Stack
          key={cell.label}
          direction="row"
          spacing={2}
          sx={(theme) => ({
            flex: 1,
            minWidth: 0,
            p: 2,
            alignItems: 'baseline',
            borderLeft: {
              xs: 'none',
              sm:
                index > 0
                  ? `1px solid ${theme.palette.stroke.default}`
                  : 'none',
            },
          })}
        >
          <Typography
            variant="bodySmRegular"
            sx={{ color: 'text.tertiary', minWidth: 140, flexShrink: 0 }}
          >
            {cell.label}
          </Typography>
          <Box sx={{ minWidth: 0 }}>
            {typeof cell.value === 'string' ||
            typeof cell.value === 'number' ? (
              <Typography
                variant="bodySmRegular"
                sx={{ color: 'text.primary' }}
              >
                {cell.value}
              </Typography>
            ) : (
              cell.value
            )}
          </Box>
        </Stack>
      ))}
    </Box>
  );
}
