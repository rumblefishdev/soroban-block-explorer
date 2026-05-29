import { Card, Skeleton, Stack, Typography } from '@mui/material';
import type { Theme } from '@mui/material/styles';
import type { ReactNode } from 'react';

type ValueVariant = 'heading2SemiBold' | 'heading3Bold' | 'heading4SemiBold';
type LabelVariant = 'bodySmMedium' | 'bodyMedium';

interface KpiCellProps {
  label: ReactNode;
  value?: ReactNode;
  caption?: ReactNode;

  valueVariant?: ValueVariant;

  labelVariant?: LabelVariant;

  valueColor?: string | ((theme: Theme) => string);
  align?: 'start' | 'center';
  loading?: boolean;

  card?: boolean;
}

export function KpiCell({
  label,
  value,
  caption,
  valueVariant = 'heading2SemiBold',
  labelVariant = 'bodySmMedium',
  valueColor,
  align = 'start',
  loading = false,
  card = true,
}: KpiCellProps) {
  const alignItems = align === 'center' ? 'center' : 'flex-start';

  const spacing = card ? 1 : 0.5;
  const inner = (
    <Stack
      spacing={spacing}
      alignItems={alignItems}
      sx={{
        flex: 1,
        width: '100%',
        minWidth: 0,
        px: card ? 2 : 0,
        py: card ? 1 : 3,
      }}
    >
      <Typography
        component="span"
        variant={labelVariant}
        sx={(theme) => ({
          color:
            labelVariant === 'bodyMedium'
              ? theme.palette.text.tertiary
              : theme.palette.text.secondary,
        })}
      >
        {label}
      </Typography>
      {loading ? (
        <Skeleton variant="text" width={140} sx={{ fontSize: 32 }} />
      ) : (
        <Typography
          component="div"
          variant={valueVariant}
          sx={(theme) => ({
            color:
              typeof valueColor === 'function'
                ? valueColor(theme)
                : valueColor ?? theme.palette.text.primary,
          })}
        >
          {value ?? '—'}
        </Typography>
      )}
      {caption != null && (
        <Typography
          component="span"
          variant="bodySmMedium"
          sx={(theme) => ({
            color: card
              ? theme.palette.text.secondary
              : theme.palette.text.tertiary,
          })}
        >
          {caption}
        </Typography>
      )}
    </Stack>
  );
  return card ? <Card sx={{ flex: 1, minWidth: 0 }}>{inner}</Card> : inner;
}
