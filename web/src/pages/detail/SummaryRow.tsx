import { Box, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

export interface SummaryCell {
  label: string;
  value: ReactNode;
}

/**
 * A flat cell list laid out two-per-row — the pairing lives HERE, beside the
 * 2-cell row contract it derives from, so callers with a variable number of
 * cells (a 3/4-leg soroban pool's reserves) don't each re-implement the
 * chunking. An odd tail renders as a single half-width cell.
 */
export function SummaryRows({ cells }: { cells: SummaryCell[] }) {
  return (
    <>
      {Array.from({ length: Math.ceil(cells.length / 2) }, (_, i) => (
        <SummaryRow key={i} cells={cells.slice(i * 2, i * 2 + 2)} />
      ))}
    </>
  );
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
        backgroundColor: theme.palette.surface.grayMain,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        '&:last-of-type': { borderBottom: 'none' },
        minHeight: 44,
      })}
    >
      {cells.map((cell, index) => (
        <Stack
          // Composite key — `cell.label` alone collides when two cells
          // share a label (e.g. fake-XLM pool legs both rendering as
          // `XLM reserve` in `PoolSummary`). Prefixing with the array
          // index guarantees uniqueness regardless of label duplication.
          key={`${index}-${cell.label}`}
          direction="row"
          spacing={2}
          sx={{
            flex: 1,
            minWidth: 0,
            px: 2,
            py: 0.75,
            alignItems: 'center',
          }}
        >
          <Typography
            variant="bodySmBold"
            sx={(theme) => ({
              color: theme.palette.text.primary,

              minWidth: { xs: 'auto', sm: 140 },
              flexShrink: 0,
            })}
          >
            {cell.label}
          </Typography>
          <Box sx={{ minWidth: 0 }}>
            {typeof cell.value === 'string' ||
            typeof cell.value === 'number' ? (
              <Typography
                variant="bodySmMedium"
                sx={(theme) => ({ color: theme.palette.text.primary })}
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
