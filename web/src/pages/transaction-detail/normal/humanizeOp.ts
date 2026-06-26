import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import {
  formatStroopAmount,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';

import { formatOperationType } from '../../transactions/operationTypes.js';

function shortId(value: string): string {
  return truncateMiddle(value, { prefix: 6, suffix: 4 });
}

function detailsObject(
  heavy: XdrOperationDto | null
): Record<string, unknown> | null {
  const details = heavy?.details;
  return details && typeof details === 'object' && !Array.isArray(details)
    ? (details as Record<string, unknown>)
    : null;
}

/** `"native"` → `XLM`; `"CODE:ISSUER"` → `CODE`; otherwise the raw string. */
function assetUnit(asset: unknown): string | null {
  if (typeof asset !== 'string' || asset.length === 0) return null;
  if (asset === 'native') return 'XLM';
  return asset.split(':')[0] ?? null;
}

/**
 * Per-op `(amount, asset)` detail keys, by operation type. Payment carries
 * `amount`/`asset`; the path payments carry the moved value under
 * `destAmount`/`destAsset` (strict-receive: exact delivered) and
 * `sendAmount`/`sendAsset` (strict-send: exact sent).
 */
const TRANSFER_FIELDS: Record<string, readonly [string, string]> = {
  PAYMENT: ['amount', 'asset'],
  PATH_PAYMENT_STRICT_RECEIVE: ['destAmount', 'destAsset'],
  PATH_PAYMENT_STRICT_SEND: ['sendAmount', 'sendAsset'],
};

/**
 * `Sent 12.5 USDC to GA5X…` for a payment / path payment, reading the per-op
 * amount + asset from the heavy XDR overlay (`operations_appearances` folds
 * and drops the amount — task 0329). Null when the heavy amount is
 * unavailable so the caller can fall back to the asset/destination-only line.
 */
function sentLineFromHeavy(
  light: OperationItem,
  heavy: XdrOperationDto | null,
  amountKey: string,
  assetKey: string
): string | null {
  const details = detailsObject(heavy);
  const amount = details?.[amountKey];
  if (typeof amount !== 'number') return null;
  const unit = assetUnit(details?.[assetKey]) ?? light.asset_code ?? 'XLM';
  const valued = formatStroopAmount(amount, unit);
  const dest =
    (typeof details?.destination === 'string' && details.destination) ||
    light.destination_account;
  return dest != null ? `Sent ${valued} to ${shortId(dest)}` : `Sent ${valued}`;
}

function fnNameFromHeavy(heavy: XdrOperationDto | null): string | null {
  const fn = detailsObject(heavy)?.function_name;
  return typeof fn === 'string' && fn.length > 0 ? fn : null;
}

function summaryFromHeavy(heavy: XdrOperationDto | null): string | null {
  const details = heavy?.details;
  if (details && typeof details === 'object' && !Array.isArray(details)) {
    const value = (details as { summary?: unknown }).summary;
    if (typeof value === 'string' && value.length > 0) return value;
  }
  return null;
}

export function humanizeOp(
  light: OperationItem,
  heavy: XdrOperationDto | null
): string {
  const explicit = summaryFromHeavy(heavy);
  if (explicit != null) return explicit;

  const opLabel = formatOperationType(light.type_name);

  const transfer = TRANSFER_FIELDS[light.type_name];
  if (transfer != null) {
    const withAmount = sentLineFromHeavy(
      light,
      heavy,
      transfer[0],
      transfer[1]
    );
    if (withAmount != null) return withAmount;
    // Heavy unavailable: asset code + destination only, no amount.
    if (light.destination_account != null) {
      const asset = light.asset_code ?? 'XLM';
      return `Sent ${asset} to ${shortId(light.destination_account)}`;
    }
    return `${opLabel} processed`;
  }

  switch (light.type_name) {
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
        return `Created account ${shortId(light.destination_account)}`;
      }
      break;
  }

  return `${opLabel} processed`;
}
