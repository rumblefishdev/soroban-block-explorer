import { Stack, Typography, type TypographyProps } from '@mui/material';
import {
  formatFee,
  formatStroops,
} from '@rumblefish/soroban-block-explorer-ui';

interface FeeCellProps {
  /** Fee in stroops. */
  stroops: number;
  /** Variant for the primary `X XLM` line. */
  primaryVariant?: TypographyProps['variant'];
  /** Variant for the secondary `(N stroops)` line. */
  secondaryVariant?: TypographyProps['variant'];
}

/**
 * Two-line fee cell: `X XLM` over a muted `(N stroops)` line. Shared by the
 * transaction-detail and ledger-detail summaries, which differ only in the
 * two text variants (weight / mono).
 */
export function FeeCell({
  stroops,
  primaryVariant = 'bodySmRegular',
  secondaryVariant = 'bodyXsRegular',
}: FeeCellProps) {
  return (
    <Stack spacing={0.25}>
      <Typography
        variant={primaryVariant}
        sx={(theme) => ({ color: theme.palette.text.primary })}
      >
        {formatFee(stroops)}
      </Typography>
      <Typography
        variant={secondaryVariant}
        sx={(theme) => ({ color: theme.palette.text.tertiary })}
      >
        ({formatStroops(stroops)} stroops)
      </Typography>
    </Stack>
  );
}
