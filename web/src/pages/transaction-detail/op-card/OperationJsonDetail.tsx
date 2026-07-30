import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import WarningAmberOutlinedIcon from '@mui/icons-material/WarningAmberOutlined';
import { Box, Stack, Typography } from '@mui/material';
import { Chip, EmptyState } from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { isSorobanOp } from '../shared/opKind.js';

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
  // Shared definition with the card's kind chip — the two must never disagree
  // on the same operation.
  if (isSorobanOp(opType)) {
    return <Chip size="sm" color="success" label="Soroban" />;
  }
  return <Chip size="sm" color="neutral" label="Classic" />;
}

function inlineScalar(value: unknown): ReactNode {
  if (typeof value === 'boolean') {
    return <Chip size="sm" color="neutral" label={String(value)} />;
  }
  if (typeof value === 'number') {
    return <Chip size="sm" color="neutral" label={String(value)} />;
  }
  if (typeof value === 'string' && value.length > 0) {
    // Short enum-ish values (native, host-function type, amounts) stay chips;
    // long identifiers (strkeys, CODE:ISSUER assets) go to wrapping mono — a
    // Chip won't break mid-word and overflows the row.
    return value.length <= 20 ? (
      <Chip size="sm" color="neutral" label={value} />
    ) : (
      <MonoText>{value}</MonoText>
    );
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

  // Soroban (INVOKE_HOST_FUNCTION) gets the typed rows above; every other op
  // type (payment, manage_offer, change_trust, …) emits its own `details` map
  // from the XDR parser — render those generically so classic ops aren't just a
  // bare chip. Scalars → inline chip, structured values → pretty JSON.
  const isInvoke = fnName != null;
  const genericEntries =
    !isInvoke &&
    details != null &&
    typeof details === 'object' &&
    !Array.isArray(details)
      ? Object.entries(details as Record<string, unknown>)
      : [];

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
      {isInvoke && (
        <AdvancedRow
          label="functionName"
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
      {genericEntries.map(([key, value]) => (
        <AdvancedRow
          key={key}
          label={key}
          value={inlineScalar(value) ?? <HighlightedJson value={value} />}
        />
      ))}
    </Box>
  );
}
