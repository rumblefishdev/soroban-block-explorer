import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import {
  DEFAULT_TRUNCATION,
  formatTokenAmount,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';

import {
  formatOperationType,
  isKnownOperationType,
} from '../../transactions/operationTypes.js';

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

/** `details.asset` + `details.amount` as one formatted string, or null when
 *  either half is missing — the shape most asset-carrying ops share. */
function fmtAssetAmount(
  details: Record<string, unknown> | null,
  fallbackUnit: string | null = null
): string | null {
  const unit = assetUnit(details?.asset, fallbackUnit);
  const amount = asAmount(details?.amount);
  return unit != null && amount != null
    ? formatTokenAmount(amount, unit)
    : null;
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
        // The 'XLM' fallback is CORRECT here, despite the doc only saying
        // "asset code for classic asset operations": on a payment a null code
        // does mean native (0377 F7).
        //
        // Do NOT re-derive this from `asset_code`/`asset_issuer_id` agreeing —
        // that proves nothing. `split_asset_ref` (persist/stage.rs) returns the
        // pair all-or-nothing, so "0 rows one-sided" is forced by the writer
        // and reads identically for a genuine native and for a parse failure.
        //
        // The discriminating evidence comes from `operation_asset_appearances`,
        // written by a separate parser path: among single-operation payment
        // transactions, EVERY blank-code one resolves to the native asset id
        // and every non-blank one does not — 11_168/11_168 and 55_582/55_582 in
        // the recent window, and the same at the oldest indexed partition.
        // Three were spot-checked against Horizon, all `asset_type: native`.
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
      const details = detailsObj(heavy);
      // The parser emits camelCase keys (`functionName`), not snake_case.
      const fn = detStr(details, 'functionName');
      if (fn != null && light.contract_id != null) {
        return `Called ${fn}() on ${shortId(light.contract_id)}`;
      }
      if (fn != null) return `Called ${fn}()`;
      // Deploy/upload variants carry no functionName — the discriminator says
      // what actually happened instead of a generic "Invoked contract".
      const hostFnType = detStr(details, 'hostFunctionType');
      if (
        hostFnType === 'createContract' ||
        hostFnType === 'createContractV2'
      ) {
        return light.contract_id != null
          ? `Deployed contract ${shortId(light.contract_id)}`
          : 'Deployed a contract';
      }
      if (hostFnType === 'uploadContractWasm') {
        const bytes = asAmount(details?.wasmLength);
        return bytes != null
          ? `Uploaded contract code (${Number(bytes).toLocaleString(
              'en-US'
            )} bytes)`
          : 'Uploaded contract code';
      }
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
    case 'LIQUIDITY_POOL_DEPOSIT':
    case 'LIQUIDITY_POOL_WITHDRAW': {
      const id = detStr(detailsObj(heavy), 'liquidityPoolId');
      if (id == null) break;
      return light.type_name === 'LIQUIDITY_POOL_DEPOSIT'
        ? `Deposited into liquidity pool ${shortId(id)}`
        : `Withdrew from liquidity pool ${shortId(id)}`;
    }
    case 'ACCOUNT_MERGE': {
      const dest =
        detStr(detailsObj(heavy), 'destination') ?? light.destination_account;
      if (dest == null) break;
      return `Merged this account into ${shortId(dest)}`;
    }
    case 'CREATE_CLAIMABLE_BALANCE': {
      const details = detailsObj(heavy);
      const formatted = fmtAssetAmount(details);
      if (formatted == null) break;
      // 0460 #16: `claimants` is the address list — name one or two
      // outright; three or more read better as a count (derived from the
      // same list, no second source).
      const destinations = Array.isArray(details?.claimants)
        ? details.claimants.flatMap((claimant) => {
            const d = (claimant as { destination?: unknown }).destination;
            return typeof d === 'string' && d.length > 0 ? [d] : [];
          })
        : [];
      const who =
        destinations.length >= 1 && destinations.length <= 2
          ? ` for ${destinations.map(shortId).join(' and ')}`
          : destinations.length > 2
          ? ` for ${destinations.length} claimants`
          : '';
      return `Escrowed ${formatted}${who}`;
    }
    case 'CLAIM_CLAIMABLE_BALANCE':
    case 'CLAWBACK_CLAIMABLE_BALANCE': {
      const details = detailsObj(heavy);
      const id = detStr(details, 'balanceId');
      if (id == null) break;
      const claim = light.type_name === 'CLAIM_CLAIMABLE_BALANCE';
      // asset + amount come from the same-op ledger entry (spec D8); absent
      // on responses parsed before that landed, or when the meta lacks the
      // entry — then the id is all we can honestly say (same as SE).
      const formatted = fmtAssetAmount(details);
      if (formatted != null) {
        return claim
          ? `Claimed ${formatted}`
          : `Clawed back escrowed ${formatted}`;
      }
      return claim
        ? `Claimed balance ${shortId(id)}`
        : `Clawed back balance ${shortId(id)}`;
    }
    case 'CLAWBACK': {
      const details = detailsObj(heavy);
      const formatted = fmtAssetAmount(details);
      const from = detStr(details, 'from');
      if (formatted == null || from == null) break;
      return `Clawed back ${formatted} from ${shortId(from)}`;
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
      return `Extended contract state TTL to at least ${Number(
        extendTo
      ).toLocaleString('en-US')} ledgers`;
    }
    case 'RESTORE_FOOTPRINT':
      return 'Restored archived contract state';
    case 'INFLATION':
      return 'Ran inflation';
  }

  if (!isKnownOperationType(light.type_name)) {
    // D2: an unknown type must never crash the page, but it must not pass
    // silently either — this is how new protocol ops get noticed.
    console.warn(`humanizeOp: no sentence template for "${light.type_name}"`);
  }
  return `${opLabel} processed`;
}
