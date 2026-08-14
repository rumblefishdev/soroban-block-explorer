import type { XdrOperationDto } from '@rumblefish/api-types';
import { Box, Stack, Typography } from '@mui/material';
import { formatTokenAmount } from '@rumblefish/soroban-block-explorer-ui';

import { assetUnit, detailsObj } from '../shared/humanizeOp.js';

export interface RouteEdge {
  /** e.g. "521.4657 TF" — what this hop paid out; null for hops whose fill is
   *  not in `claimedAtoms` (order-book segments are LP-invisible, spec D9). */
  label: string | null;
}

export interface RouteModel {
  /** The DECLARED chain: send asset, every path hop, dest asset — always the
   *  complete route, never reduced to what the atoms happened to cover. */
  chips: string[];
  /** Edges between chips; edges.length === chips.length - 1. */
  edges: RouteEdge[];
  /** True when at least one hop has no pool fill. */
  partial: boolean;
}

interface Atom {
  assetSold?: unknown;
  amountSold?: unknown;
}

/** Build the route strip model for a path payment. Chips always come from the
 *  declared `sendAsset → path… → destAsset`; per-hop amounts overlay from
 *  `claimedAtoms` where the fill's output matches the next chip in sequence.
 *  The sequential match is deliberately conservative: if atom order ever
 *  disagrees with the declared chain (e.g. an unexpected execution order),
 *  amounts simply stay off rather than attach to the wrong hop. Returns null
 *  when the op is not a path payment with a decodable send/dest pair. */
export function buildRouteModel(
  heavy: XdrOperationDto | null
): RouteModel | null {
  const details = detailsObj(heavy);
  if (details == null) return null;
  const sendUnit = assetUnit(details.sendAsset, null);
  const destUnit = assetUnit(details.destAsset, null);
  if (sendUnit == null || destUnit == null) return null;

  const hops = Array.isArray(details.path)
    ? details.path
        .map((asset) => assetUnit(asset, null))
        .filter((code): code is string => code != null)
    : [];
  const chips = [sendUnit, ...hops, destUnit];

  const atoms: Atom[] = Array.isArray(details.claimedAtoms)
    ? (details.claimedAtoms as Atom[])
    : [];
  let atomIndex = 0;
  const edges: RouteEdge[] = chips.slice(1).map((target) => {
    const atom = atoms[atomIndex];
    const output = atom != null ? assetUnit(atom.assetSold, null) : null;
    if (output === target && typeof atom?.amountSold === 'number') {
      atomIndex += 1;
      return { label: formatTokenAmount(atom.amountSold, output) };
    }
    return { label: null };
  });

  return {
    chips,
    edges,
    partial: edges.some((edge) => edge.label == null),
  };
}

/** `applied` (tx.successful) picks the right explanation for missing edge
 *  amounts: on an applied tx they crossed the order book (those fills are
 *  not in the LP-only atoms — D9); on a failed tx nothing executed at all,
 *  so blaming the order book would be a lie. */
export function RouteStrip({
  model,
  applied,
}: {
  model: RouteModel;
  applied: boolean;
}) {
  return (
    <Box sx={{ mt: 1.25, overflowX: 'auto' }}>
      <Stack
        direction="row"
        spacing={1}
        alignItems="center"
        sx={{ width: 'max-content' }}
      >
        {model.chips.map((chip, index) => {
          const edge = model.edges[index - 1];
          return (
            <Stack
              key={`${chip}-${index}`}
              direction="row"
              spacing={1}
              alignItems="center"
            >
              {index > 0 && (
                <Stack alignItems="center" sx={{ minWidth: 56 }}>
                  <Typography
                    variant="bodyXsRegular"
                    sx={(theme) => ({ color: theme.palette.text.tertiary })}
                  >
                    →
                  </Typography>
                  {edge?.label != null && (
                    <Typography
                      variant="bodyMonoSmRegular"
                      sx={(theme) => ({
                        color: theme.palette.text.secondary,
                        whiteSpace: 'nowrap',
                      })}
                    >
                      {edge.label}
                    </Typography>
                  )}
                </Stack>
              )}
              <Box
                sx={(theme) => ({
                  px: 1.25,
                  py: 0.25,
                  borderRadius: `${theme.shape.radius.s}px`,
                  backgroundColor: theme.palette.surface.information,
                  whiteSpace: 'nowrap',
                })}
              >
                <Typography
                  variant="bodyMonoSmMedium"
                  sx={(theme) => ({ color: theme.palette.text.primary })}
                >
                  {chip}
                </Typography>
              </Box>
            </Stack>
          );
        })}
      </Stack>
      {model.partial && (
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({ color: theme.palette.text.tertiary, mt: 0.5 })}
        >
          {applied
            ? 'Hops without an amount crossed the order book — those fills are not in the pool data this page reads yet.'
            : 'Route as signed — the transaction failed, so no exchange was executed.'}
        </Typography>
      )}
    </Box>
  );
}
