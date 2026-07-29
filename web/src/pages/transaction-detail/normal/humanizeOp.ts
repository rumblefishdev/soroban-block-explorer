import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import {
  DEFAULT_TRUNCATION,
  formatTokenAmount,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';

import { formatOperationType } from '../../transactions/operationTypes.js';

export function shortId(value: string): string {
  return truncateMiddle(value, DEFAULT_TRUNCATION);
}

/** Heavy `details` as a plain object, or null when absent / not an object. */
export function detailsObj(
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
export function assetUnit(
  value: unknown,
  fallback: string | null
): string | null {
  if (typeof value === 'string' && value.length > 0) {
    if (value === 'native') return 'XLM';
    const code = value.split(':')[0];
    if (code.length > 0) return code;
  }
  return fallback;
}

/** True when the operation pays its own source — wallets route swaps as
 *  self-payments, and "to GAFB…36GD" is meaningless for those. An operation
 *  without its own source inherits the transaction's, so the caller passes
 *  that as the fallback. */
function isSelf(light: OperationItem, txSource: string | null): boolean {
  const source = light.source_account ?? txSource;
  return source != null && source === light.destination_account;
}

/** i64::MAX stroops arrives lossy through JSON numbers; anything this close to
 *  the top of the i64 range can only mean "no limit". */
const UNLIMITED_STROOPS = 9.2e18;

function detStr(
  details: Record<string, unknown> | null,
  key: string
): string | null {
  const v = details?.[key];
  return typeof v === 'string' && v.length > 0 ? v : null;
}

/** Offer price is the XDR rational `{n, d}`. */
function priceStr(value: unknown): string | null {
  if (value == null || typeof value !== 'object') return null;
  const { n, d } = value as { n?: unknown; d?: unknown };
  if (typeof n !== 'number' || typeof d !== 'number' || d === 0) return null;
  return (n / d).toLocaleString('en-US', { maximumFractionDigits: 7 });
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

/** One true sentence per operation type, built from the heavy `details` the
 *  API already delivers (light fields as degraded fallback). Wording adapted
 *  from stellar.expert's open-source explorer (stellar-expert/ui-framework,
 *  `tx/op-description-view.js`, MIT). */
export function humanizeOp(
  light: OperationItem,
  heavy: XdrOperationDto | null,
  txSourceAccount: string | null = null
): string {
  const opLabel = formatOperationType(light.type_name);

  switch (light.type_name) {
    case 'PAYMENT':
      if (light.destination_account != null) {
        const details = detailsObj(heavy);
        const unit = assetUnit(details?.asset, light.asset_code ?? 'XLM');
        const amount = asAmount(details?.amount);
        const target = isSelf(light, txSourceAccount)
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
      if (
        !isSelf(light, txSourceAccount) &&
        light.destination_account != null
      ) {
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
    case 'MANAGE_SELL_OFFER':
    case 'MANAGE_BUY_OFFER':
    case 'CREATE_PASSIVE_SELL_OFFER': {
      const details = detailsObj(heavy);
      const selling = assetUnit(details?.selling, null);
      const buying = assetUnit(details?.buying, null);
      if (details == null || selling == null || buying == null) break;
      const buySide = light.type_name === 'MANAGE_BUY_OFFER';
      const amount = asAmount(details[buySide ? 'buyAmount' : 'amount']);
      const offerId = asAmount(details.offerId);
      const offerIdNum = offerId != null ? Number(offerId) : null;
      if (
        offerIdNum != null &&
        offerIdNum !== 0 &&
        amount != null &&
        Number(amount) === 0
      ) {
        return `Cancelled offer #${offerIdNum}`;
      }
      const amountStr =
        amount != null
          ? formatTokenAmount(amount, buySide ? buying : selling)
          : buySide
          ? buying
          : selling;
      // XDR price is always "units of selling per 1 unit of buying" for buy
      // offers and "units of buying per 1 unit of selling" for sell offers.
      const price = priceStr(details.price);
      const priceSuffix =
        price != null
          ? buySide
            ? ` @ ${price} ${selling}/${buying}`
            : ` @ ${price} ${buying}/${selling}`
          : '';
      const action = buySide
        ? `buy ${amountStr} for ${selling}`
        : `sell ${amountStr} for ${buying}`;
      if (light.type_name === 'CREATE_PASSIVE_SELL_OFFER') {
        return `Placed a passive offer: ${action}${priceSuffix}`;
      }
      if (offerIdNum != null && offerIdNum !== 0) {
        return `Updated offer #${offerIdNum}: ${action}${priceSuffix}`;
      }
      return `Offered to ${action}${priceSuffix}`;
    }
    case 'LIQUIDITY_POOL_DEPOSIT': {
      const id = detStr(detailsObj(heavy), 'liquidityPoolId');
      if (id == null) break;
      return `Deposited into liquidity pool ${shortId(id)}`;
    }
    case 'LIQUIDITY_POOL_WITHDRAW': {
      const id = detStr(detailsObj(heavy), 'liquidityPoolId');
      if (id == null) break;
      return `Withdrew from liquidity pool ${shortId(id)}`;
    }
    case 'ACCOUNT_MERGE': {
      const dest =
        detStr(detailsObj(heavy), 'destination') ?? light.destination_account;
      if (dest == null) break;
      return `Merged this account into ${shortId(dest)}`;
    }
    case 'CREATE_CLAIMABLE_BALANCE': {
      const details = detailsObj(heavy);
      const unit = assetUnit(details?.asset, null);
      const amount = asAmount(details?.amount);
      if (unit == null || amount == null) break;
      const claimants = asAmount(details?.claimants);
      const who =
        claimants != null
          ? ` for ${claimants} claimant${Number(claimants) === 1 ? '' : 's'}`
          : '';
      return `Escrowed ${formatTokenAmount(amount, unit)}${who}`;
    }
    case 'CLAIM_CLAIMABLE_BALANCE': {
      const id = detStr(detailsObj(heavy), 'balanceId');
      if (id == null) break;
      // The asset is not in the operation body; naming it needs the
      // ledger-entry changes (spec D8). Until then: id only, like SE.
      return `Claimed balance ${shortId(id)}`;
    }
    case 'CLAWBACK_CLAIMABLE_BALANCE': {
      const id = detStr(detailsObj(heavy), 'balanceId');
      if (id == null) break;
      return `Clawed back balance ${shortId(id)}`;
    }
    case 'CLAWBACK': {
      const details = detailsObj(heavy);
      const unit = assetUnit(details?.asset, null);
      const amount = asAmount(details?.amount);
      const from = detStr(details, 'from');
      if (unit == null || amount == null || from == null) break;
      return `Clawed back ${formatTokenAmount(amount, unit)} from ${shortId(
        from
      )}`;
    }
    case 'SET_TRUST_LINE_FLAGS':
    case 'ALLOW_TRUST': {
      const details = detailsObj(heavy);
      const trustor = detStr(details, 'trustor');
      const code = assetUnit(details?.asset, null);
      if (trustor == null || code == null) break;
      const who = shortId(trustor);
      // AUTHORIZED flag is bit 1 in both ops' flag fields.
      const authorized =
        light.type_name === 'ALLOW_TRUST'
          ? Number(asAmount(details?.authorize) ?? 0) & 1
          : Number(asAmount(details?.setFlags) ?? 0) & 1;
      const authorizeField = asAmount(details?.authorize);
      const revoked =
        light.type_name === 'ALLOW_TRUST'
          ? authorizeField != null && Number(authorizeField) === 0
          : (Number(asAmount(details?.clearFlags) ?? 0) & 1) === 1;
      if (authorized) return `Authorized ${who} for ${code}`;
      if (revoked) return `Revoked ${who}'s authorization for ${code}`;
      return `Updated trustline flags for ${who} on ${code}`;
    }
    case 'BEGIN_SPONSORING_FUTURE_RESERVES': {
      const sponsored = detStr(detailsObj(heavy), 'sponsoredId');
      if (sponsored == null) break;
      return `Sponsored reserves for ${shortId(sponsored)}`;
    }
    case 'END_SPONSORING_FUTURE_RESERVES':
      return 'Ended reserve sponsorship';
    case 'REVOKE_SPONSORSHIP': {
      const details = detailsObj(heavy);
      const kind = detStr(details, 'kind');
      if (kind === 'signer') {
        const account = detStr(details, 'accountId');
        return account != null
          ? `Revoked sponsorship of a signer on ${shortId(account)}`
          : 'Revoked sponsorship of a signer';
      }
      if (kind === 'ledgerEntry') {
        const entry = detStr(details, 'ledgerKeyType');
        return entry != null
          ? `Revoked sponsorship of a ${entry} entry`
          : 'Revoked sponsorship of a ledger entry';
      }
      break;
    }
    case 'SET_OPTIONS': {
      const details = detailsObj(heavy);
      if (details == null) break;
      const signerKey = detStr(details, 'signerKey');
      if (signerKey != null) {
        const weight = asAmount(details.signerWeight);
        return weight != null && Number(weight) === 0
          ? `Removed signer ${shortId(signerKey)}`
          : `Set signer ${shortId(signerKey)}${
              weight != null ? ` (weight ${weight})` : ''
            }`;
      }
      const homeDomain = detStr(details, 'homeDomain');
      if (homeDomain != null) return `Set home domain to ${homeDomain}`;
      return 'Updated account options';
    }
    case 'MANAGE_DATA': {
      const details = detailsObj(heavy);
      const name = detStr(details, 'name');
      if (details == null || name == null) break;
      return 'value' in details && details.value === null
        ? `Deleted data entry "${name}"`
        : `Set data entry "${name}"`;
    }
    case 'BUMP_SEQUENCE': {
      const bumpTo = asAmount(detailsObj(heavy)?.bumpTo);
      if (bumpTo == null) break;
      return `Bumped sequence to ${bumpTo}`;
    }
    case 'EXTEND_FOOTPRINT_TTL': {
      const extendTo = asAmount(detailsObj(heavy)?.extendTo);
      if (extendTo == null) break;
      return `Extended contract state TTL by ${Number(extendTo).toLocaleString(
        'en-US'
      )} ledgers`;
    }
    case 'RESTORE_FOOTPRINT':
      return 'Restored archived contract state';
    case 'INFLATION':
      return 'Ran inflation';
  }

  return `${opLabel} processed`;
}
