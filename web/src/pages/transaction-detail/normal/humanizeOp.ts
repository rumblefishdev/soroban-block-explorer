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

function asObject(value: unknown): Record<string, unknown> | null {
  return value != null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function num(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function str(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

/** `"native"` → `XLM`; `"CODE:ISSUER"` → `CODE`; otherwise the raw string. */
function assetUnit(asset: unknown): string | null {
  if (typeof asset !== 'string' || asset.length === 0) return null;
  if (asset === 'native') return 'XLM';
  return asset.split(':')[0] ?? null;
}

/** Stellar `price` is an exact `{n, d}` rational; render it as a decimal. */
function priceDecimal(price: unknown): string | null {
  const p = asObject(price);
  const n = num(p?.n);
  const d = num(p?.d);
  if (n == null || d == null || d === 0) return null;
  return String(Number((n / d).toPrecision(6)));
}

/**
 * `Sent 12.5 USDC to GA5X…` for a payment / path payment, reading the per-op
 * amount + asset from the heavy XDR overlay (`operations_appearances` folds
 * and drops the amount — task 0329). Null when the heavy amount is
 * unavailable so the caller can fall back to the asset/destination-only line.
 */
function sentLine(
  light: OperationItem,
  details: Record<string, unknown> | null,
  amountKey: string,
  assetKey: string
): string | null {
  const amount = num(details?.[amountKey]);
  if (amount == null) return null;
  const unit = assetUnit(details?.[assetKey]) ?? light.asset_code ?? 'XLM';
  const valued = formatStroopAmount(amount, unit);
  const dest = str(details?.destination) ?? light.destination_account;
  return dest != null ? `Sent ${valued} to ${shortId(dest)}` : `Sent ${valued}`;
}

/**
 * Offer summary, e.g. `Sell offer: 100 XLM for USDC @ 0.5`. `ownAssetKey` is
 * the asset whose `amountKey` is denominated; `otherAssetKey` is the
 * counter-asset. Null when the amount is unavailable.
 */
function offerLine(
  details: Record<string, unknown> | null,
  amountKey: string,
  ownAssetKey: string,
  otherAssetKey: string,
  label: string
): string | null {
  const amount = num(details?.[amountKey]);
  if (amount == null) return null;
  const own = assetUnit(details?.[ownAssetKey]);
  const other = assetUnit(details?.[otherAssetKey]);
  // amount 0 deletes/cancels the offer — show that, not "0 ASSET".
  if (amount === 0) {
    const pair = own != null && other != null ? ` (${own}/${other})` : '';
    return `Cancelled ${label.toLowerCase()}${pair}`;
  }
  const price = priceDecimal(details?.price);
  let line = `${label}: ${formatStroopAmount(amount, own ?? '')}`;
  if (other != null) line += ` for ${other}`;
  if (price != null) line += ` @ ${price}`;
  return line;
}

/** Fallback `Sent ASSET to DEST` (no amount) from light fields. */
function assetDestFallback(light: OperationItem): string | null {
  if (light.destination_account == null) return null;
  return `Sent ${light.asset_code ?? 'XLM'} to ${shortId(
    light.destination_account
  )}`;
}

function fnNameFromHeavy(heavy: XdrOperationDto | null): string | null {
  return str(detailsObject(heavy)?.function_name);
}

function summaryFromHeavy(heavy: XdrOperationDto | null): string | null {
  return str(detailsObject(heavy)?.summary);
}

/**
 * One-line human summary of an operation for the Result node. Amounts come
 * from the heavy XDR overlay (`details`) — the folded light rows carry no
 * token amount (task 0329). Falls back to a light-only line, then to
 * `<Type> processed`, when heavy is unavailable.
 */
export function humanizeOp(
  light: OperationItem,
  heavy: XdrOperationDto | null
): string {
  const explicit = summaryFromHeavy(heavy);
  if (explicit != null) return explicit;

  const details = detailsObject(heavy);
  const opLabel = formatOperationType(light.type_name);
  const processed = `${opLabel} processed`;

  switch (light.type_name) {
    case 'PAYMENT':
      return (
        sentLine(light, details, 'amount', 'asset') ??
        assetDestFallback(light) ??
        processed
      );
    case 'PATH_PAYMENT_STRICT_RECEIVE':
      return (
        sentLine(light, details, 'destAmount', 'destAsset') ??
        assetDestFallback(light) ??
        processed
      );
    case 'PATH_PAYMENT_STRICT_SEND':
      return (
        sentLine(light, details, 'sendAmount', 'sendAsset') ??
        assetDestFallback(light) ??
        processed
      );
    case 'CREATE_ACCOUNT': {
      const dest = str(details?.destination) ?? light.destination_account;
      if (dest == null) break;
      const balance = num(details?.startingBalance);
      return balance != null
        ? `Created account ${shortId(dest)} with ${formatStroopAmount(
            balance,
            'XLM'
          )}`
        : `Created account ${shortId(dest)}`;
    }
    case 'CLAWBACK': {
      const amount = num(details?.amount);
      if (amount == null) break;
      const valued = formatStroopAmount(
        amount,
        assetUnit(details?.asset) ?? 'XLM'
      );
      const from = str(details?.from);
      return from != null
        ? `Clawed back ${valued} from ${shortId(from)}`
        : `Clawed back ${valued}`;
    }
    case 'CREATE_CLAIMABLE_BALANCE': {
      const amount = num(details?.amount);
      if (amount == null) break;
      const unit = assetUnit(details?.asset) ?? 'XLM';
      return `Created claimable balance of ${formatStroopAmount(amount, unit)}`;
    }
    case 'MANAGE_SELL_OFFER':
      return (
        offerLine(details, 'amount', 'selling', 'buying', 'Sell offer') ??
        processed
      );
    case 'CREATE_PASSIVE_SELL_OFFER':
      return (
        offerLine(details, 'amount', 'selling', 'buying', 'Passive sell') ??
        processed
      );
    case 'MANAGE_BUY_OFFER':
      return (
        offerLine(details, 'buyAmount', 'buying', 'selling', 'Buy offer') ??
        processed
      );
    case 'LIQUIDITY_POOL_DEPOSIT': {
      // No asset codes in the deposit op — bare 7-decimal amounts.
      const a = num(details?.maxAmountA);
      const b = num(details?.maxAmountB);
      if (a == null || b == null) break;
      return `Deposited up to ${formatStroopAmount(
        a,
        ''
      )} / ${formatStroopAmount(b, '')}`;
    }
    case 'LIQUIDITY_POOL_WITHDRAW': {
      const shares = num(details?.amount);
      if (shares == null) break;
      return `Withdrew ${formatStroopAmount(shares, 'pool shares')}`;
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
  }

  return processed;
}
