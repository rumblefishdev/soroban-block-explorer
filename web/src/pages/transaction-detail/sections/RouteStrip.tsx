import type { XdrOperationDto } from '@rumblefish/api-types';
import { Box, Stack, Typography } from '@mui/material';
import { formatTokenAmount } from '@rumblefish/soroban-block-explorer-ui';

import { assetUnit, detailsObj } from '../normal/humanizeOp.js';

export interface RouteEdge {
  /** e.g. "521.4657 TF" — what came out of this hop. */
  label: string | null;
}

export interface RouteModel {
  /** Asset chips: send asset, one per pool hop output, dest asset. */
  chips: string[];
  /** Edges between chips; edges.length === chips.length - 1. */
  edges: RouteEdge[];
  /** True when the route also crossed the order book — those fills are not
   *  in `claimedAtoms` (LP-only), so some hop amounts are unknowable here
   *  (spec D9/D10; the effects engine of D14 is the real fix). */
  partial: boolean;
}

interface Atom {
  assetSold?: unknown;
  amountSold?: unknown;
  assetBought?: unknown;
  amountBought?: unknown;
}

/** Build the pool-crossing route from `claimedAtoms`. Returns null when there
 *  are no atoms (direct payment, order-book-only route, or failed tx) — the
 *  card then falls back to the plain-text Route fact from `opFacts`. */
export function buildRouteModel(
  heavy: XdrOperationDto | null
): RouteModel | null {
  const details = detailsObj(heavy);
  if (details == null) return null;
  const atoms = Array.isArray(details.claimedAtoms)
    ? (details.claimedAtoms as Atom[])
    : [];
  if (atoms.length === 0) return null;

  const sendUnit = assetUnit(details.sendAsset, null);
  const destUnit = assetUnit(details.destAsset, null);
  if (sendUnit == null || destUnit == null) return null;

  const chips: string[] = [sendUnit];
  const edges: RouteEdge[] = [];
  for (const atom of atoms) {
    const bought = assetUnit(atom.assetSold, null);
    const amount =
      typeof atom.amountSold === 'number' && bought != null
        ? formatTokenAmount(atom.amountSold, bought)
        : null;
    // Each LP atom is one pool crossing; `assetSold`/`amountSold` is the side
    // the pool paid out (what the taker received from this hop).
    chips.push(bought ?? '?');
    edges.push({ label: amount });
  }
  // The declared destination closes the chain; if the last atom already ends
  // on the destination asset the chip run stays as-is, otherwise the tail is
  // an order-book segment claimedAtoms cannot see.
  const partial = chips[chips.length - 1] !== destUnit;
  if (partial) {
    chips.push(destUnit);
    edges.push({ label: null });
  }
  const hops = Array.isArray(details.path) ? details.path.length : 0;
  return { chips, edges, partial: partial || atoms.length < hops + 1 };
}

export function RouteStrip({ model }: { model: RouteModel }) {
  return (
    <Box sx={{ mt: 1.25, overflowX: 'auto' }}>
      <Stack
        direction="row"
        spacing={1}
        alignItems="center"
        sx={{ width: 'max-content' }}
      >
        {model.chips.map((chip, index) => (
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
                {model.edges[index - 1]?.label != null && (
                  <Typography
                    variant="bodyMonoSmRegular"
                    sx={(theme) => ({
                      color: theme.palette.text.secondary,
                      whiteSpace: 'nowrap',
                    })}
                  >
                    {model.edges[index - 1].label}
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
        ))}
      </Stack>
      {model.partial && (
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({ color: theme.palette.text.tertiary, mt: 0.5 })}
        >
          Route partly crossed the order book — those hop amounts are not
          available yet.
        </Typography>
      )}
    </Box>
  );
}
