import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import { Box, Stack, Typography } from '@mui/material';
import { Chip } from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { HighlightedJson } from './HighlightedJson.js';

interface OperationJsonDetailProps {
  light: OperationItem;
  heavy: XdrOperationDto | null;
}

function pickDetailValue(
  details: unknown,
  key: string
): { present: boolean; value: unknown } {
  if (
    details != null &&
    typeof details === 'object' &&
    !Array.isArray(details) &&
    key in (details as Record<string, unknown>)
  ) {
    return { present: true, value: (details as Record<string, unknown>)[key] };
  }
  return { present: false, value: undefined };
}

function asString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function MonoText({ children }: { children: ReactNode }) {
  return (
    <Typography
      component="span"
      variant="bodyMonoSmMedium"
      sx={(theme) => ({
        color: theme.palette.text.primary,
        wordBreak: 'break-all',
      })}
    >
      {children}
    </Typography>
  );
}

function AdvancedRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <Stack
      direction="row"
      spacing={2}
      alignItems="flex-start"
      sx={(theme) => ({
        px: 2,
        py: 1.5,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        '&:last-of-type': { borderBottom: 'none' },
      })}
    >
      <Typography
        component="span"
        variant="bodySmSemiBold"
        sx={(theme) => ({
          color: theme.palette.text.primary,
          minWidth: 180,
          flexShrink: 0,
          pt: 0.25,
        })}
      >
        {label}
      </Typography>
      <Box sx={{ minWidth: 0, flex: 1 }}>{value}</Box>
    </Stack>
  );
}

function categoryChip(opType: string): ReactNode {
  if (opType === 'invoke_host_function') {
    return <Chip size="sm" color="success" label="Soroban" />;
  }
  return <Chip size="sm" color="neutral" label="Classic" />;
}

function inlineScalar(value: unknown): ReactNode {
  if (typeof value === 'boolean') {
    return <Chip size="sm" color="neutral" label={String(value)} />;
  }
  if (typeof value === 'string' && value.length > 0 && value.length <= 64) {
    return <Chip size="sm" color="neutral" label={value} />;
  }
  if (typeof value === 'number') {
    return <Chip size="sm" color="neutral" label={String(value)} />;
  }
  return null;
}

function HeavyUnavailable() {
  return (
    <Box sx={{ p: 2 }}>
      <Typography
        variant="bodySmRegular"
        sx={(theme) => ({ color: theme.palette.text.tertiary })}
      >
        Raw operation details are unavailable — heavy XDR fields could not be
        loaded for this transaction.
      </Typography>
    </Box>
  );
}

export function OperationJsonDetail({
  light,
  heavy,
}: OperationJsonDetailProps) {
  if (heavy == null) return <HeavyUnavailable />;

  const opType = heavy.op_type ?? light.type_name.toLowerCase();
  const details = heavy.details;

  const fnName = asString(pickDetailValue(details, 'function_name').value);
  const argsField = pickDetailValue(details, 'arguments');
  const returnField = pickDetailValue(details, 'return_value');
  const authField = pickDetailValue(details, 'auth');

  return (
    <Box
      sx={(theme) => ({
        borderRadius: `${theme.shape.radius.md}px`,
        border: `1px solid ${theme.palette.stroke.default}`,
        overflow: 'hidden',
        backgroundColor: theme.palette.surface.background,
      })}
    >
      <AdvancedRow label={opType} value={categoryChip(opType)} />
      {light.contract_id != null && (
        <AdvancedRow
          label="contract_id"
          value={<MonoText>{light.contract_id}</MonoText>}
        />
      )}
      {fnName != null && (
        <AdvancedRow
          label="function_name"
          value={<Chip size="sm" color="neutral" label={fnName} />}
        />
      )}
      {argsField.present && (
        <AdvancedRow
          label="arguments"
          value={<HighlightedJson value={argsField.value} />}
        />
      )}
      {returnField.present && (
        <AdvancedRow
          label="return_value"
          value={
            inlineScalar(returnField.value) ?? (
              <HighlightedJson value={returnField.value} />
            )
          }
        />
      )}
      {authField.present && (
        <AdvancedRow
          label="auth"
          value={<HighlightedJson value={authField.value} />}
        />
      )}
    </Box>
  );
}
