import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import {
  DEFAULT_TRUNCATION,
  formatTokenAmount,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';

import { formatOperationType } from '../../transactions/operationTypes.js';

function shortId(value: string): string {
  return truncateMiddle(value, DEFAULT_TRUNCATION);
}

/** Heavy `details` as a plain object, or null when absent / not an object. */
function detailsObj(
  heavy: XdrOperationDto | null
): Record<string, unknown> | null {
  const d = heavy?.details;
  return d != null && typeof d === 'object' && !Array.isArray(d)
    ? (d as Record<string, unknown>)
    : null;
}

/** A stroop amount usable by `formatTokenAmount`: a finite number or an
 *  all-digits string. The API currently serializes `details` amounts as JSON
 *  numbers, so values above 2^53 stroops (~900M XLM) are already lossy on
 *  arrival; the string branch keeps precision only if the wire form is later
 *  switched to a string (see task 0330 Future Work). */
function asAmount(value: unknown): string | number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string' && /^\d+$/.test(value.trim()))
    return value.trim();
  return null;
}

/** Maps a heavy `details` asset (`"native"` or `"CODE:ISSUER"`) to its display
 *  unit, falling back to the DB-side `asset_code` (or XLM) when absent. */
function assetUnit(value: unknown, fallback: string | null): string | null {
  if (typeof value === 'string' && value.length > 0) {
    if (value === 'native') return 'XLM';
    const code = value.split(':')[0];
    if (code.length > 0) return code;
  }
  return fallback;
}

/** True when the operation pays its own source — wallets route swaps as
 *  self-payments, and "to GAFB…36GD" is meaningless for those. */
function isSelf(light: OperationItem): boolean {
  return (
    light.source_account != null &&
    light.source_account === light.destination_account
  );
}

/** i64::MAX stroops arrives lossy through JSON numbers; anything this close to
 *  the top of the i64 range can only mean "no limit". */
const UNLIMITED_STROOPS = 9.2e18;

function fnNameFromHeavy(heavy: XdrOperationDto | null): string | null {
  const details = heavy?.details;
  if (details && typeof details === 'object' && !Array.isArray(details)) {
    // The parser emits camelCase keys (`functionName`), not snake_case.
    const fn = (details as { functionName?: unknown }).functionName;
    if (typeof fn === 'string' && fn.length > 0) return fn;
  }
  return null;
}

export function humanizeOp(
  light: OperationItem,
  heavy: XdrOperationDto | null
): string {
  const opLabel = formatOperationType(light.type_name);

  switch (light.type_name) {
    case 'PAYMENT':
      if (light.destination_account != null) {
        const details = detailsObj(heavy);
        const unit = assetUnit(details?.asset, light.asset_code ?? 'XLM');
        const amount = asAmount(details?.amount);
        const target = isSelf(light)
          ? 'itself'
          : shortId(light.destination_account);
        // Prefer the precise "amount + asset" from the heavy XDR block; fall
        // back to the asset-only label when heavy is unavailable (degraded
        // response) or the amount field is missing.
        const formatted =
          amount != null ? formatTokenAmount(amount, unit) : null;
        return formatted != null
          ? `Sent ${formatted} to ${target}`
          : `Sent ${unit ?? 'XLM'} to ${target}`;
      }
      break;
    case 'PATH_PAYMENT_STRICT_SEND':
    case 'PATH_PAYMENT_STRICT_RECEIVE': {
      const details = detailsObj(heavy);
      const sendUnit = assetUnit(details?.sendAsset, light.asset_code ?? null);
      const destUnit = assetUnit(details?.destAsset, null);
      if (details == null || sendUnit == null || destUnit == null) break;
      const strictSend = light.type_name === 'PATH_PAYMENT_STRICT_SEND';
      // strict-send commits the exact SENT amount (received is only bounded by
      // destMin); strict-receive commits the exact DELIVERED amount (spend is
      // bounded by sendMax). Never report only the send leg as a payment.
      const exact = asAmount(details[strictSend ? 'sendAmount' : 'destAmount']);
      const bound = asAmount(details[strictSend ? 'destMin' : 'sendMax']);
      const exactStr =
        exact != null
          ? formatTokenAmount(exact, strictSend ? sendUnit : destUnit)
          : null;
      let sentence = strictSend
        ? `Swapped ${exactStr ?? sendUnit} → ${destUnit}`
        : `Swapped ${sendUnit} → ${exactStr ?? destUnit}`;
      if (bound != null) {
        sentence += strictSend
          ? ` (min ${formatTokenAmount(bound, destUnit)})`
          : ` (max ${formatTokenAmount(bound, sendUnit)})`;
      }
      if (!isSelf(light) && light.destination_account != null) {
        sentence += ` for ${shortId(light.destination_account)}`;
      }
      return sentence;
    }
    case 'CHANGE_TRUST': {
      const details = detailsObj(heavy);
      const asset = details?.asset;
      if (typeof asset === 'string' && asset !== 'native') {
        const [code, issuer] = asset.split(':');
        const suffix = issuer ? ` (issuer ${shortId(issuer)})` : '';
        const limit = asAmount(details?.limit);
        if (limit != null) {
          const limitNum = Number(limit);
          if (limitNum === 0) return `Removed trustline to ${code}${suffix}`;
          if (limitNum < UNLIMITED_STROOPS) {
            return `Set trustline to ${code}${suffix} · limit ${formatTokenAmount(
              limit,
              code
            )}`;
          }
        }
        return `Set trustline to ${code}${suffix}`;
      }
      if (asset != null && typeof asset === 'object') {
        // Pool-share trustline: the parser keeps only the XDR union arm name,
        // so the pool's asset pair is not available here.
        const poolLimit = asAmount(details?.limit);
        return poolLimit != null && Number(poolLimit) === 0
          ? 'Removed trustline to liquidity pool shares'
          : 'Set trustline to liquidity pool shares';
      }
      // Degraded (heavy unavailable): light still carries the asset identity.
      if (light.asset_code != null) {
        const suffix =
          light.asset_issuer != null
            ? ` (issuer ${shortId(light.asset_issuer)})`
            : '';
        return `Set trustline to ${light.asset_code}${suffix}`;
      }
      break;
    }
    case 'INVOKE_HOST_FUNCTION': {
      const fn = fnNameFromHeavy(heavy);
      if (fn != null && light.contract_id != null) {
        return `Called ${fn}() on ${shortId(light.contract_id)}`;
      }
      if (fn != null) return `Called ${fn}()`;
      if (light.contract_id != null) {
        return `Invoked contract ${shortId(light.contract_id)}`;
      }
      break;
    }
    case 'CREATE_ACCOUNT':
      if (light.destination_account != null) {
        const dest = shortId(light.destination_account);
        const amount = asAmount(detailsObj(heavy)?.startingBalance);
        const formatted =
          amount != null ? formatTokenAmount(amount, 'XLM') : null;
        return formatted != null
          ? `Created account ${dest} with ${formatted}`
          : `Created account ${dest}`;
      }
      break;
  }

  return `${opLabel} processed`;
}
