import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import KeyboardArrowRightIcon from '@mui/icons-material/KeyboardArrowRight';
import { Box, Collapse, Stack, Typography } from '@mui/material';
import { Chip } from '@rumblefish/soroban-block-explorer-ui';
import { useState } from 'react';

import { formatOperationType } from '../../transactions/operationTypes.js';
import { OperationJsonDetail } from '../advanced/OperationJsonDetail.js';
import { detailsObj, humanizeOp } from '../normal/humanizeOp.js';

import { opFacts } from './opFacts.js';
import { OpIcon } from './opIcon.js';

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
}

export function OperationCard({
  light,
  heavy,
  applied,
  defaultDetailsOpen,
  fallbackOrder,
  txSourceAccount,
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
  const facts = opFacts(light, heavy);
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
