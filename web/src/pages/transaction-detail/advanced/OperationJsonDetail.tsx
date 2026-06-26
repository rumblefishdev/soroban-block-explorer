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

function detailEntries(details: unknown): [string, unknown][] {
  return details != null &&
    typeof details === 'object' &&
    !Array.isArray(details)
    ? Object.entries(details as Record<string, unknown>)
    : [];
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

/** Scalars as a chip / mono text; objects & arrays as highlighted JSON. */
function renderValue(value: unknown): ReactNode {
  if (value === null) return <MonoText>null</MonoText>;
  if (typeof value === 'object') return <HighlightedJson value={value} />;
  return inlineScalar(value) ?? <MonoText>{String(value)}</MonoText>;
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
  const entries = detailEntries(heavy.details);

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
      {entries.map(([key, value]) => (
        <AdvancedRow key={key} label={key} value={renderValue(value)} />
      ))}
    </Box>
  );
}
