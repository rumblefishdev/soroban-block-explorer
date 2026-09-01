import type {
  PoolAssetLeg,
  PoolItem,
  PoolLegItem,
} from '@rumblefish/api-types';

import { scaleByDecimals } from '@rumblefish/soroban-block-explorer-ui';

import { assetColor } from '../assets/assetColor.js';
import { NATIVE_ASSET_CODE } from '../assets/assetType.js';
import { routes } from '../../router/routes.js';

/**
 * Resolve the cross-entity link target for a pool asset leg (task 0263).
 * Always routes to the asset detail page — backend `parse_asset_id`
 * accepts either the SAC C-strkey or a `code-issuer` composite, so both
 * classic and SAC legs resolve to the same asset row.
 *
 * Precedence:
 *   1. `asset_type === 0` (native XLM) → `/assets/native`. The reserved
 *      `native` literal IS the canonical asset token (task 0243) — the older
 *      "native has no on-chain address, so no link" rule predates it and left
 *      XLM as the only unlinkable leg in the app, while account balances,
 *      search and the SAC chip all route there (task 0472).
 *   2. `asset_code` + `issuer` (classic credit) → `/assets/${code}-${issuer}`.
 *      Preferred over the SAC C-address ON PURPOSE: task 0364 dropped the
 *      SAC-facet aliasing arm from `fetch_by_contract_id` (asset_type is
 *      pinned to 3), so `/assets/{SAC C…}` 404s — verified against the API
 *      2026-08-13. The earlier contract_id-first order sent ~93k classic
 *      legs to a dead page.
 *   3. `contract_id` → `/assets/${contract_id}`. Only reachable when the
 *      code/issuer pair is incomplete; resolves for a genuine Soroban token
 *      contract, 404s for a bare SAC address — a best-effort last resort.
 *   4. Anything else (schema drift) → no link.
 */
export function legHref(leg: PoolAssetLeg): string | undefined {
  if (leg.asset_type === 0) return routes.asset('native');
  if (leg.asset_code && leg.issuer) {
    return routes.asset(`${leg.asset_code}-${leg.issuer}`);
  }
  if (leg.contract_id) return routes.asset(leg.contract_id);
  return undefined;
}

/**
 * Returns the display label for one leg of a pool's asset pair.
 *
 * Native (XLM) legs come back with `asset_type_name === 'native'` and
 * `null` `asset_code`. Classic, SAC, and Soroban legs all carry a code.
 *
 * **Hard-fail on schema drift.** If a leg has neither the native flag
 * nor an `asset_code` the backend contract is broken — throw rather
 * than silently render a `?` placeholder, so the bug is caught by the
 * surrounding `SectionErrorBoundary` instead of leaking into the UI.
 */
export function assetLegLabel(leg: PoolAssetLeg): string {
  if (leg.asset_type_name === 'native') return NATIVE_ASSET_CODE;
  if (leg.asset_code != null && leg.asset_code !== '') return leg.asset_code;
  throw new Error(
    `assetLegLabel: non-native leg has no asset_code (asset_type_name=${
      leg.asset_type_name ?? 'null'
    })`
  );
}

// ---------------------------------------------------------------------------
// Unified leg views (task 0374): classic pools carry an `asset_a`/`asset_b`
// PAIR, soroban pools carry 2–4 `legs[]`. Every pool surface renders through
// this one view so the two shapes cannot drift apart per component.
// ---------------------------------------------------------------------------

export interface PoolLegView {
  /** Display label — code / symbol / truncated contract / explicit `?`. */
  label: string;
  /** Asset-detail link, when the leg resolves to a linkable identity. */
  href?: string;
  /** Reserve-dot / KPI colour, keyed by label like the avatars. */
  dotColor: string;
  /** Leg avatar icon (classic legs only — soroban tokens have none yet). */
  iconUrl?: string | null;
  /**
   * Display-ready decimal reserve. Classic: pre-scaled DB-side
   * (Decimal128(7) → string). Soroban: raw units scaled here by the leg's
   * on-chain `decimals`; `null` when the reserve OR the scale is unknown —
   * an unknown scale must never render a raw integer as if it were scaled.
   */
  reserve: string | null;
  /**
   * Scale for RAW amounts of this leg (activity feed): the on-chain token
   * `decimals` for a soroban leg, `7` for every classic leg (stroops).
   * `null` when unknown — callers must skip, never show raw units.
   */
  decimals: number | null;
}

export function isSorobanPool(pool: Pick<PoolItem, 'pool_kind'>): boolean {
  return pool.pool_kind === 'soroban';
}

/**
 * Display label for a SOROBAN pool leg. Precedence: native → XLM; classic
 * credit → its code; soroban token → on-chain symbol, else truncated
 * contract; unresolved → literal `?` — kept EXPLICIT on purpose (house
 * rule: no plausible-looking fallback for a leg we failed to resolve).
 */
export function legItemLabel(leg: PoolLegItem): string {
  if (leg.family === 'native') return NATIVE_ASSET_CODE;
  // Classic identity outranks a metadata symbol: a classic-credit leg's
  // code IS its name everywhere else in the app; symbols only name
  // bespoke soroban tokens (their code is empty).
  if (leg.asset_code) return leg.asset_code;
  if (leg.symbol) return leg.symbol;
  if (leg.contract_id) {
    return `${leg.contract_id.slice(0, 4)}…${leg.contract_id.slice(-4)}`;
  }
  return '?';
}

/** Asset-detail link for a soroban pool leg — same precedence family as
 *  [`legHref`]: native token page / classic composite / token contract. */
export function legItemHref(leg: PoolLegItem): string | undefined {
  if (leg.family === 'native') return routes.asset('native');
  if (leg.asset_code && leg.issuer) {
    return routes.asset(`${leg.asset_code}-${leg.issuer}`);
  }
  if (leg.family === 'soroban' && leg.contract_id) {
    return routes.asset(leg.contract_id);
  }
  return undefined;
}

/**
 * The pool's legs as uniform views, either world.
 *
 * Classic rows without their pair legs are a broken backend contract —
 * throw into the section boundary rather than render a half-pool.
 */
export function poolLegViews(pool: PoolItem): PoolLegView[] {
  if (isSorobanPool(pool)) {
    return (pool.legs ?? []).map((leg) => {
      const label = legItemLabel(leg);
      return {
        label,
        href: legItemHref(leg),
        dotColor: assetColor(label).dot,
        reserve:
          leg.reserve != null && leg.decimals != null
            ? scaleByDecimals(leg.reserve, leg.decimals)
            : null,
        decimals: leg.decimals ?? null,
      };
    });
  }
  if (pool.asset_a == null || pool.asset_b == null) {
    throw new Error('poolLegViews: classic pool without its asset pair');
  }
  return (
    [
      [pool.asset_a, pool.reserve_a],
      [pool.asset_b, pool.reserve_b],
    ] as const
  ).map(([leg, reserve]) => {
    const label = assetLegLabel(leg);
    return {
      label,
      href: legHref(leg),
      dotColor: assetColor(label).dot,
      iconUrl: leg.icon_url,
      reserve: reserve ?? null,
      decimals: 7,
    };
  });
}

/**
 * `XLM / USDC`, or `XLM / AQUA / USDx` — the pool's display name, either
 * world. Named for LEGS, not a pair, because it joins however many the pool
 * has: 3- and 4-leg stable pools are live on mainnet (measured: 491 pools
 * carry two legs, 7 carry three, 2 carry four). The old `poolPairLabel`
 * carried the one assumption this model exists to remove.
 */
export function poolLegsLabel(pool: PoolItem): string {
  return poolLegViews(pool)
    .map((l) => l.label)
    .join(' / ');
}
