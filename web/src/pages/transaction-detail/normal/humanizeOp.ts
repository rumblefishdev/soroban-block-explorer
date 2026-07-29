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

/** The (amount field, asset field) in heavy `details` that best represents the
 *  value moved, per payment variant. PATH_PAYMENT_STRICT_SEND carries no actual
 *  destination amount (only `destMin`), so it falls back to what the source
 *  committed (`sendAmount`/`sendAsset`); STRICT_RECEIVE uses the delivered
 *  `destAmount`/`destAsset`; plain PAYMENT uses `amount`/`asset`. */
function amountFieldsFor(typeName: string): { amount: string; asset: string } {
  switch (typeName) {
    case 'PATH_PAYMENT_STRICT_SEND':
      return { amount: 'sendAmount', asset: 'sendAsset' };
    case 'PATH_PAYMENT_STRICT_RECEIVE':
      return { amount: 'destAmount', asset: 'destAsset' };
    default:
      return { amount: 'amount', asset: 'asset' };
  }
}

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
    case 'PATH_PAYMENT_STRICT_RECEIVE':
    case 'PATH_PAYMENT_STRICT_SEND':
      if (light.destination_account != null) {
        const details = detailsObj(heavy);
        const fields = amountFieldsFor(light.type_name);
        const unit = assetUnit(
          details?.[fields.asset],
          light.asset_code ?? 'XLM'
        );
        const amount = asAmount(details?.[fields.amount]);
        const dest = shortId(light.destination_account);
        // Prefer the precise "amount + asset" from the heavy XDR block; fall
        // back to the asset-only label when heavy is unavailable (degraded
        // response) or the amount field is missing.
        const formatted =
          amount != null ? formatTokenAmount(amount, unit) : null;
        // `unit` already folds in `light.asset_code` (then 'XLM') as its
        // fallback, so the asset-only branch just needs `unit`.
        return formatted != null
          ? `Sent ${formatted} to ${dest}`
          : `Sent ${unit ?? 'XLM'} to ${dest}`;
      }
      break;
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
