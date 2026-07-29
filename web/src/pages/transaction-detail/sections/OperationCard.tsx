import type {
  OperationItem,
  XdrEventDto,
  XdrOperationDto,
} from '@rumblefish/api-types';
import KeyboardArrowRightIcon from '@mui/icons-material/KeyboardArrowRight';
import { Box, Collapse, Stack, Typography } from '@mui/material';
import { Chip } from '@rumblefish/soroban-block-explorer-ui';
import { useState } from 'react';

import { formatOperationType } from '../../transactions/operationTypes.js';
import { OperationJsonDetail } from '../advanced/OperationJsonDetail.js';
import { detailsObj, humanizeOp } from '../normal/humanizeOp.js';

import { CallTree, parseOperationTree } from './CallTree.js';
import { opFacts } from './opFacts.js';
import { OpIcon } from './opIcon.js';
import { buildRouteModel, RouteStrip } from './RouteStrip.js';

const SOROBAN_TYPES = new Set([
  'INVOKE_HOST_FUNCTION',
  'EXTEND_FOOTPRINT_TTL',
  'RESTORE_FOOTPRINT',
]);

interface OperationCardProps {
  light: OperationItem | undefined;
  heavy: XdrOperationDto | null;
  /** `tx.successful` — Stellar is atomic, so a failed transaction means no
   *  operation was applied (the summary banner states the verdict; the card
   *  dims and labels itself). */
  applied: boolean;
  /** Advanced mode opens the raw-details section by default. */
  defaultDetailsOpen: boolean;
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

export function OperationCard({
  light,
  heavy,
  applied,
  defaultDetailsOpen,
  fallbackOrder,
  txSourceAccount,
  operationTree,
  contractEvents = [],
}: OperationCardProps) {
  const [detailsOpen, setDetailsOpen] = useState(defaultDetailsOpen);

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

  const order = light.application_order ?? fallbackOrder;
  const label = formatOperationType(light.type_name);
  const kind = SOROBAN_TYPES.has(light.type_name) ? 'Soroban' : 'Classic';
  const routeModel = buildRouteModel(heavy);
  // The strip shows the route with per-hop amounts; drop the plain-text
  // Route row so the same chain is not stated twice.
  const facts = opFacts(light, heavy).filter(
    (fact) => routeModel == null || fact.label !== 'Route'
  );
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
        <Box
          sx={(theme) => ({
            width: 32,
            height: 32,
            borderRadius: '50%',
            backgroundColor: theme.palette.blue[100],
            color: theme.palette.blue[600],
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
            fontSize: 16,
          })}
        >
          <OpIcon typeName={light.type_name} />
        </Box>
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({
            color: theme.palette.text.tertiary,
            textTransform: 'uppercase',
            letterSpacing: '0.05em',
            fontWeight: 650,
          })}
        >
          {order} · {label}
        </Typography>
        <Chip
          size="sm"
          color={kind === 'Soroban' ? 'success' : 'neutral'}
          label={kind}
        />
        {!applied && (
          <Box
            sx={(theme) => ({
              ml: 'auto',
              px: 1,
              borderRadius: `${theme.shape.radius.s}px`,
              border: `1px solid ${theme.palette.stroke.error}`,
            })}
          >
            <Typography
              variant="bodyXsRegular"
              sx={(theme) => ({ color: theme.palette.text.error })}
            >
              not applied
            </Typography>
          </Box>
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
            <Typography
              variant="bodyXsRegular"
              sx={(theme) => ({
                color: theme.palette.text.tertiary,
                textTransform: 'uppercase',
                letterSpacing: '0.05em',
                fontWeight: 650,
                mb: 0.5,
              })}
            >
              Call tree
            </Typography>
            <CallTree nodes={callNodes} />
          </Box>
        )}

        {opEvents.length > 0 && (
          <Box sx={{ mt: 1.25 }}>
            <Typography
              variant="bodyXsRegular"
              sx={(theme) => ({
                color: theme.palette.text.tertiary,
                textTransform: 'uppercase',
                letterSpacing: '0.05em',
                fontWeight: 650,
                mb: 0.5,
              })}
            >
              Events · {opEvents.length}
            </Typography>
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
                    {event.contract_id.slice(0, 4)}…
                    {event.contract_id.slice(-4)}
                  </Typography>
                )}
              </Stack>
            ))}
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

      <Box
        role="button"
        tabIndex={0}
        aria-expanded={detailsOpen}
        onClick={() => setDetailsOpen((open) => !open)}
        onKeyDown={(event) => {
          if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault();
            setDetailsOpen((open) => !open);
          }
        }}
        sx={(theme) => ({
          display: 'flex',
          alignItems: 'center',
          gap: 0.75,
          mt: 1.5,
          pt: 1.25,
          borderTop: `1px solid ${theme.palette.stroke.default}`,
          cursor: 'pointer',
          color: theme.palette.text.secondary,
        })}
      >
        <KeyboardArrowRightIcon
          sx={{
            fontSize: 18,
            transform: detailsOpen ? 'rotate(90deg)' : 'none',
            transition: 'transform 120ms ease',
          }}
        />
        <Typography variant="bodySmSemiBold" sx={{ color: 'inherit' }}>
          Operation details
        </Typography>
        {detailCount > 0 && (
          <Chip size="sm" color="neutral" label={String(detailCount)} />
        )}
      </Box>
      <Collapse in={detailsOpen} unmountOnExit>
        <Box sx={{ mt: 1.5 }}>
          <OperationJsonDetail light={light} heavy={heavy} />
        </Box>
      </Collapse>
    </Box>
  );
}
