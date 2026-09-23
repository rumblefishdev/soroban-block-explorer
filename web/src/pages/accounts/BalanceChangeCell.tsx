import { Box, Link, Stack, Tooltip, Typography } from '@mui/material';
import type { AccountBalanceChange } from '@rumblefish/api-types';
import {
  contentLinkSx,
  formatAmount,
  scaleByDecimals,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
import { Link as RouterLink } from 'react-router-dom';

import { routes } from '../../router/routes.js';
import { assetDisplayCode, isNativeAssetString } from '../assets/assetType.js';

/** The dimmed weight this cell uses for everything that is not a movement. */
function Muted({
  children,
  variant = 'bodySmRegular',
}: {
  children: ReactNode;
  variant?: 'bodySmRegular' | 'bodyXsRegular';
}) {
  return (
    <Typography
      component="span"
      variant={variant}
      sx={(theme) => ({ color: theme.palette.text.tertiary })}
    >
      {children}
    </Typography>
  );
}

/**
 * `Balance change` cell — what one transaction did to THE ACCOUNT WHOSE PAGE
 * THIS IS (task 0540). The figure is account-relative by construction: the same
 * transaction opened from another account shows different numbers, which is why
 * this column exists here and nowhere else. Its predecessor put a number with no
 * account behind it on the global list, and that is exactly what made it wrong.
 *
 * Two states, both measurements — the per-transfer index covers every
 * transaction the API lists:
 *
 * | `changes`  | Means                                    | Renders        |
 * | ---------- | ---------------------------------------- | -------------- |
 * | `[]`       | NO TOKEN moved for this account           | `0`            |
 * | non-empty  | these assets moved                        | signed amounts |
 *
 * `[]` does not claim the XLM balance held: a fee is charged whatever happens,
 * and the fee is the column next door. This column is about tokens that moved.
 *
 * The header carries the meaning of the sign — a BALANCE CHANGE is a result,
 * not a transfer, so a net of two-way movement cannot be misread as a one-way
 * payment and needs no extra glyph. Assets that net to zero are already dropped
 * server-side, so `+N` counts assets that actually changed.
 */
export function BalanceChangeCell({
  changes,
}: {
  changes: AccountBalanceChange[];
}) {
  // A MEASURED zero: the transaction is indexed and NO TOKEN MOVED for this
  // account — an offer placed, a round-trip that netted out, a payment to
  // self. It does NOT claim the XLM balance held: the fee is charged whatever
  // happens, and the fee lives in its own column beside this one. Dimmed,
  // because most transactions on an account's list move no tokens and full
  // contrast would shout over the rows that did. NOT a dash: a dash means
  // "missing", and this is a measurement.
  if (changes.length === 0) {
    return <Muted>0</Muted>;
  }

  const [first] = changes;
  const others = changes.length - 1;
  const cell = (
    <Box sx={{ display: 'inline-flex', alignItems: 'baseline', gap: 0.5 }}>
      <ChangeAmount change={first} />
      <AssetLink change={first} />
      {others > 0 && <Muted variant="bodyXsRegular">+{others}</Muted>}
    </Box>
  );
  if (others === 0) return cell;

  // The column shows the FIRST movement and collapses the rest into `+N`,
  // which expands on hover. The order is the chain's own — the API returns each
  // transaction's assets in the order their movements occurred — and is
  // rendered as given: assets have different decimals and different prices, and
  // no price exists anywhere here, so any ranking by amount would compare
  // quantities that are not comparable. Tooltip links inherit the inverted
  // surface's text colour; the page accent goes unreadable there.
  return (
    <Tooltip
      title={
        <Stack spacing={0.25}>
          {changes.map((change, i) => (
            // Index, not the asset: an entry with no link identity carries an
            // EMPTY asset, and two such tokens in one transaction would
            // collide on any content-derived key. The list is a fixed,
            // non-reorderable render of one API response, so the index is
            // stable for its lifetime.
            <span key={i}>
              <ChangeAmount change={change} inverted />{' '}
              <AssetLink change={change} inverted />
            </span>
          ))}
        </Stack>
      }
    >
      {cell}
    </Tooltip>
  );
}

/**
 * The signed figure. Positive is money in, negative is money out, and the
 * colour repeats what the sign already says rather than replacing it.
 *
 * A `null` amount is NOT zero — it is a NON-FUNGIBLE movement, where no amount
 * exists by nature (the event carries a token id, not a value). The piece
 * changed hands, so the cell says how many pieces and in which direction, and
 * never prints a `0` that would claim nothing happened. When the API could name
 * the single piece that moved it is shown too (`+1 NFT #44`); where it could
 * not, the count stands alone rather than guessing an id.
 */
function ChangeAmount({
  change,
  inverted = false,
}: {
  change: AccountBalanceChange;
  inverted?: boolean;
}) {
  // `nft_delta`, not `amount == null` — the same signal `AssetLink` uses. The
  // two are equal by the read's own grouping, and reading one field in both
  // places means a slip in that invariant cannot show a number here while
  // linking to a collection there.
  const nonFungible = change.nft_delta !== 0;
  const raw = change.amount ?? String(change.nft_delta);
  const negative = raw.startsWith('-');
  const magnitude = raw.replace(/^-/, '');
  // `scaleByDecimals` takes UNSIGNED raw integers (a negative returns null, by
  // its own contract), so the sign is split off here and re-attached — rather
  // than widening a shared formatter every other caller depends on.
  const figure = nonFungible
    ? magnitude
    : formatAmount(scaleByDecimals(magnitude, change.decimals), 2);
  return (
    <Typography
      component="span"
      variant="bodySmRegular"
      sx={(theme) => ({
        color: inverted
          ? 'inherit'
          : negative
          ? theme.palette.text.error
          : theme.palette.text.success,
      })}
    >
      {negative ? '−' : '+'}
      {figure}
      {nonFungible && ' NFT'}
      {nonFungible && change.token_id && ` #${change.token_id}`}
    </Typography>
  );
}

/**
 * The asset the figure is denominated in, linked to the page that can actually
 * show it. Three destinations, most specific first:
 *
 * 1. **The PIECE** (`/nfts/:contract/:tokenId`) when the API named one —
 *    exactly one piece of that collection moved in that transaction.
 * 2. **The COLLECTION** (`/nfts?contract=…`) for any other non-fungible
 *    movement: several pieces at once, or a collection whose ownership rows
 *    are not indexed. Coarser, but never a confident wrong answer.
 * 3. **The ASSET page** for anything fungible.
 *
 * Never `/assets/C…` for a non-fungible movement: an NFT collection has no
 * `assets` row — that is the whole reason the read joins it `LEFT` — so that
 * URL answers `404` for exactly the collections this cell names.
 *
 * An asset with no link identity at all renders as plain text rather than as a
 * link that goes nowhere.
 *
 * **Not `IdentifierDisplay`**, which wraps this same `RouterLink` +
 * `contentLinkSx` pairing: it renders at weight 500 in the mono identifier
 * face, which would make the asset code louder than the amount beside it and
 * invert this cell's amount / code / count hierarchy. The shared affordance
 * this cell owes (task 0535) is the underline, and `contentLinkSx` is where
 * that lives — so the affordance is shared even though the component is not.
 */
function AssetLink({
  change,
  inverted = false,
}: {
  change: AccountBalanceChange;
  inverted?: boolean;
}) {
  const label = assetLabel(change);
  if (!change.asset) {
    return (
      <Typography
        component="span"
        variant="bodySmRegular"
        sx={(theme) => ({
          color: inverted ? 'inherit' : theme.palette.text.tertiary,
        })}
      >
        {label}
      </Typography>
    );
  }
  // `nft_delta` is the fungibility signal, not `amount`: the two are separate
  // entries by construction (the read groups on it), so this is a total split
  // with no third case.
  let href;
  if (!change.nft_delta) {
    href = routes.asset(change.asset);
  } else if (change.token_id) {
    href = routes.nft(change.asset, change.token_id);
  } else {
    href = routes.nftsByContract(change.asset);
  }
  return (
    <Link
      component={RouterLink}
      to={href}
      variant="bodySmRegular"
      underline={inverted ? 'always' : undefined}
      sx={(theme) => ({
        color: inverted ? 'inherit' : theme.palette.surface.primaryMainAlt,
        ...(inverted ? {} : contentLinkSx(theme)),
      })}
    >
      {label}
    </Link>
  );
}

/**
 * Display code — the app-wide {@link assetDisplayCode} ladder, adapted to the
 * shape an operation carries: the asset arrives as a STRING (`'native'` |
 * `'CODE:ISSUER'` | a `C…` contract StrKey) rather than as an asset row, so
 * the native rung is fed from `isNativeAssetString` instead of a family name
 * and the contract rung from that same string.
 *
 * Feeding the contract rung is what makes this row agree with the asset page:
 * a token with no classic code and no on-chain symbol reads `CB2T…3B5R` in
 * both, instead of a marker here and its address there. 605 of 4 463 soroban
 * assets publish no symbol (production, 2026-09-22).
 *
 * The marker survives for the one case that has nothing left: `asset` is EMPTY
 * when the API refuses the link (no `assets` row, so `/assets/{id}` answers
 * 404), and then the row carries no code, no symbol and no address — 66 tokens
 * and 274 transfers on production, all fungible; a non-fungible row keeps its
 * address.
 *
 * It says UNREGISTERED, not "unnamed" and not a dash: the movement IS indexed,
 * and the token is one the registry never got a row for, because that row comes
 * from the classifier's guess at the WASM's function names rather than from the
 * evidence that the contract moved an amount (task 0542).
 */
function assetLabel(change: AccountBalanceChange): string {
  const native = isNativeAssetString(change.asset);
  return (
    assetDisplayCode({
      asset_type_name: native ? 'native' : null,
      asset_code: change.asset_code,
      contract_id: native ? null : change.asset,
    }) ?? 'Unregistered token'
  );
}
