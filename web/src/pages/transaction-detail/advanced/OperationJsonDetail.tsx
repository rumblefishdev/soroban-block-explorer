import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import WarningAmberOutlinedIcon from '@mui/icons-material/WarningAmberOutlined';
import { Box, Stack, Typography } from '@mui/material';
import { Chip, EmptyState } from '@rumblefish/soroban-block-explorer-ui';
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
        variant="bodySmRegular"
        sx={(theme) => ({
          color: theme.palette.text.primary,
          minWidth: 140,
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
  // op_type arrives SCREAMING_SNAKE (`INVOKE_HOST_FUNCTION`); compare lowercased
  // so the Soroban chip actually fires (was stuck on "Classic").
  if (opType.toLowerCase() === 'invoke_host_function') {
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
    <EmptyState
      icon={<WarningAmberOutlinedIcon />}
      variant="warning"
      title="Raw operation details unavailable"
      description="Heavy XDR fields could not be loaded for this transaction."
      py={4}
    />
  );
}

export function OperationJsonDetail({
  light,
  heavy,
}: OperationJsonDetailProps) {
  if (heavy == null) return <HeavyUnavailable />;

  const opType = heavy.op_type ?? light.type_name.toLowerCase();
  const details = heavy.details;

  // Keys are the camelCase shape the XDR parser emits for INVOKE_HOST_FUNCTION
  // (`extract_invoke_host_function`): functionName / functionArgs / returnValue.
  // (Previously read snake_case `function_name`/`arguments`/`return_value`, which
  // never matched → the decoded call was silently dropped from the UI.)
  const fnName = asString(pickDetailValue(details, 'functionName').value);
  const argsField = pickDetailValue(details, 'functionArgs');
  const returnField = pickDetailValue(details, 'returnValue');
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
