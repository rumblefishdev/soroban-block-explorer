import { isAssetId } from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';

/**
 * The asset page for a fully-qualified `CODE:ISSUER` or `CODE-ISSUER`, or
 * `null` for anything else (task 0534).
 *
 * A qualified pair names exactly one asset, so the assets-list search sends
 * the reader there instead of filtering by it. As a filter it is a dead end:
 * `filter[code]` substring-matches the displayed code, name and symbol, no
 * code is longer than 12 characters, and the pair is at least 58 — the list
 * would come back empty.
 *
 * Split on the LAST `:` or `-`, the backend classifier's rule
 * (`search::classifier::split_code_issuer`): a code is alphanumeric and a
 * G-StrKey is base32, so neither separator occurs inside either half. The
 * issuer is upper-cased, so a lower-cased paste still routes; the code keeps
 * its case, because Stellar asset codes are case-sensitive.
 *
 * ponytail: the issuer is shape-checked (`isAssetId`), not CRC-checked like
 * the backend classifier. Here both failures are dead ends of the same cost —
 * a typo'd issuer lands on the asset page's not-found state instead of an
 * empty list — and a CRC check needs a base32 decoder the frontend has no
 * other use for (`strkeyDecode` only encodes). Add one if a bad pair should
 * ever fall back to the code filter instead.
 */
export function codeIssuerRoute(value: string): string | null {
  const q = value.trim();
  const sep = Math.max(q.lastIndexOf(':'), q.lastIndexOf('-'));
  if (sep <= 0) return null;
  const id = `${q.slice(0, sep)}-${q.slice(sep + 1).toUpperCase()}`;
  return isAssetId(id) ? routes.asset(id) : null;
}
