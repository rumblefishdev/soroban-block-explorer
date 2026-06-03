import { Box, Typography } from '@mui/material';
import { formatPercent } from '@rumblefish/soroban-block-explorer-ui';

/**
 * Green success pill used for the AMM fee — list-page rows and the
 * detail-page header. Matches Figma node `266:35469` (list) and
 * `325:7098` (detail): `surface.success` fill, `text.success` label,
 * leading 8px dot, 8px radius.
 *
 * Detail page prefixes the label with `Fee` (e.g. `Fee 0.30%`); list
 * uses the bare percentage. `prefix` toggles between the two.
 *
 * `raw` is the raw NUMERIC string the backend ships
 * (`0.300000000000000000`). We render two decimals, the canonical AMM
 * precision — extra zeros are noise.
 */
export function FeePill({
  raw,
  prefix = false,
}: {
  raw: string;
  prefix?: boolean;
}) {
  const text = formatPercent(Number(raw), raw);
  return (
    <Box
      component="span"
      sx={(theme) => ({
        display: 'inline-flex',
        alignItems: 'center',
        gap: 0.5,
        px: 1,
        py: '2px',
        borderRadius: '8px',
        backgroundColor: theme.palette.surface.success,
        color: theme.palette.text.success,
      })}
    >
      <Box
        component="span"
        sx={{
          width: 8,
          height: 8,
          borderRadius: '50%',
          bgcolor: 'currentColor',
        }}
      />
      <Typography
        component="span"
        variant="bodySmMedium"
        sx={{ color: 'inherit' }}
      >
        {prefix ? `Fee ${text}` : text}
      </Typography>
    </Box>
  );
}
