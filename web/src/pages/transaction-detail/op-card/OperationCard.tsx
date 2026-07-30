import type {
  OperationItem,
  XdrEventDto,
  XdrOperationDto,
} from '@rumblefish/api-types';
import { Box, Collapse, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';
import { Chip, IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';
import { useState } from 'react';

import { formatOperationType } from '../../transactions/operationTypes.js';
import { OperationJsonDetail } from './OperationJsonDetail.js';
import { detailsObj, humanizeOp, shortId } from '../shared/humanizeOp.js';
import { DisclosureRow } from '../shared/DisclosureRow.js';
import { isSorobanOp } from '../shared/opKind.js';

import { CallTree, parseOperationTree } from './CallTree.js';
import { opFacts } from './opFacts.js';
import { OpAvatar } from './opIcon.js';
import { buildRouteModel, RouteStrip } from './RouteStrip.js';

interface OperationCardProps {
  light: OperationItem | undefined;
  heavy: XdrOperationDto | null;
  /** `tx.successful` — Stellar is atomic, so a failed transaction means no
   *  operation was applied (the summary banner states the verdict; the card
   *  dims and labels itself). */
  applied: boolean;
  fallbackOrder: number;
  /** Ops without their own source inherit the transaction's (self-detection). */
  txSourceAccount: string | null;
  /** `heavy.operation_tree` — tx-level, safe to attach to the invoke card
   *  (protocol 21+: one InvokeHostFunction per transaction). */
  operationTree?: unknown;
  /** Tx-level `heavy.contract_events`; the card shows the ones whose
   *  `op_index` points at this operation (D7). Absent index → tx-level
   *  events section only. */
  contractEvents?: readonly XdrEventDto[];
}

/** First topic is the event name for well-formed token events. */
function eventLabel(event: XdrEventDto): string {
  const first = event.topics[0];
  if (
    first != null &&
    typeof first === 'object' &&
    (first as { type?: unknown }).type === 'sym' &&
    typeof (first as { value?: unknown }).value === 'string'
  ) {
    return (first as { value: string }).value;
  }
  return event.event_type;
}

/** Small uppercase section label used across the card. */
function Overline({ children, mb }: { children: ReactNode; mb?: number }) {
  return (
    <Typography
      variant="bodyXsRegular"
      sx={(theme) => ({
        color: theme.palette.text.tertiary,
        textTransform: 'uppercase',
        letterSpacing: '0.05em',
        fontWeight: 650,
        mb,
      })}
    >
      {children}
    </Typography>
  );
}

export function OperationCard({
  light,
  heavy,
  applied,
  fallbackOrder,
  txSourceAccount,
  operationTree,
  contractEvents = [],
}: OperationCardProps) {
  const [detailsOpen, setDetailsOpen] = useState(false);

  if (light == null) {
    return (
      <Typography
        variant="bodySmRegular"
        sx={(theme) => ({ color: theme.palette.text.tertiary, p: 2 })}
      >
        No operation selected.
      </Typography>
    );
  }

  // heavy is 1:1 with the envelope; the light row is folded, so its
  // application_order is the FIRST of the fold — wrong for later copies.
  const order =
    heavy?.application_order ?? light.application_order ?? fallbackOrder;
  const label = formatOperationType(light.type_name);
  const soroban = isSorobanOp(light.type_name);
  const routeModel = buildRouteModel(heavy);
  const facts = opFacts(light, heavy);
  const callNodes =
    light.type_name === 'INVOKE_HOST_FUNCTION'
      ? parseOperationTree(operationTree)
      : [];
  // op_index is the 0-based envelope position (CAP-67 V4 attribution);
  // responses parsed before the field landed simply match nothing.
  const opEvents =
    heavy?.application_order != null
      ? contractEvents.filter(
          (event) => event.op_index === heavy.application_order - 1
        )
      : [];
  const detailCount = Object.keys(detailsObj(heavy) ?? {}).length;

  return (
    <Box
      sx={(theme) => ({
        border: `1px solid ${theme.palette.stroke.default}`,
        borderRadius: `${theme.shape.radius.md}px`,
        p: 2,
      })}
    >
      <Stack direction="row" spacing={1.25} alignItems="center">
        <OpAvatar typeName={light.type_name} />
        <Overline>
          {order} · {label}
        </Overline>
        <Chip
          size="sm"
          color={soroban ? 'success' : 'neutral'}
          label={soroban ? 'Soroban' : 'Classic'}
        />
        {!applied && (
          <Chip
            size="sm"
            color="error"
            label="not applied"
            sx={{ ml: 'auto' }}
          />
        )}
      </Stack>

      <Box sx={{ opacity: applied ? 1 : 0.7 }}>
        <Typography
          variant="bodyMedium"
          sx={(theme) => ({ color: theme.palette.text.primary, mt: 1.25 })}
        >
          {humanizeOp(light, heavy, txSourceAccount)}
        </Typography>

        {routeModel != null && <RouteStrip model={routeModel} />}

        {callNodes.length > 0 && (
          <Box sx={{ mt: 1.25 }}>
            {/* "Authorized" is load-bearing: the tree is built from the
                transaction's auth entries (intent), not an execution trace —
                see CallNode.successful for why no per-node verdict shows. */}
            <Overline mb={0.5}>Authorized calls</Overline>
            <CallTree nodes={callNodes} />
          </Box>
        )}

        {opEvents.length > 0 && (
          <Box sx={{ mt: 1.25 }}>
            <Overline mb={0.5}>Events · {opEvents.length}</Overline>
            {opEvents.map((event) => (
              <Stack
                key={event.event_index}
                direction="row"
                spacing={1}
                alignItems="center"
                sx={{ py: 0.25 }}
              >
                <Chip size="sm" color="neutral" label={eventLabel(event)} />
                {event.contract_id != null && (
                  <Typography
                    variant="bodyMonoSmRegular"
                    sx={(theme) => ({ color: theme.palette.text.secondary })}
                  >
                    {shortId(event.contract_id)}
                  </Typography>
                )}
              </Stack>
            ))}
          </Box>
        )}

        {light.pool_ids.length > 0 && (
          <Box sx={{ mt: 1.25 }}>
            <Overline mb={0.5}>
              Pools crossed · {light.pool_ids.length}
            </Overline>
            <Stack direction="row" spacing={1.5} sx={{ flexWrap: 'wrap' }}>
              {light.pool_ids.map((poolId) => (
                <IdentifierDisplay key={poolId} value={poolId} type="pool" />
              ))}
            </Stack>
          </Box>
        )}

        {facts.length > 0 && (
          <Box
            component="dl"
            sx={{
              display: 'grid',
              gridTemplateColumns: 'minmax(96px, auto) 1fr',
              columnGap: 2,
              rowGap: 0.5,
              m: 0,
              mt: 1.25,
            }}
          >
            {facts.map((fact) => (
              <Box key={fact.label} sx={{ display: 'contents' }}>
                <Typography
                  component="dt"
                  variant="bodySmRegular"
                  sx={(theme) => ({ color: theme.palette.text.tertiary })}
                >
                  {fact.label}
                </Typography>
                <Typography
                  component="dd"
                  variant="bodySmRegular"
                  sx={(theme) => ({ color: theme.palette.text.primary, m: 0 })}
                >
                  {fact.value}
                </Typography>
              </Box>
            ))}
          </Box>
        )}
      </Box>

      <DisclosureRow
        open={detailsOpen}
        onToggle={() => setDetailsOpen((open) => !open)}
        label="Operation details"
        trailing={
          detailCount > 0 ? (
            <Chip size="sm" color="neutral" label={String(detailCount)} />
          ) : undefined
        }
        sx={(theme) => ({
          mt: 1.5,
          pt: 1.25,
          borderTop: `1px solid ${theme.palette.stroke.default}`,
        })}
      />
      <Collapse in={detailsOpen} unmountOnExit>
        <Box sx={{ mt: 1.5 }}>
          <OperationJsonDetail light={light} heavy={heavy} />
        </Box>
      </Collapse>
    </Box>
  );
}
