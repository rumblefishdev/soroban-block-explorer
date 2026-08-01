import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import { Fragment } from 'react';
import { IdentifierWithCopy } from '@rumblefish/soroban-block-explorer-ui';

import { isValidStrkey } from '../op-card/strkeyDecode.js';
import { detailsObj, humanizeOp, shortId } from './humanizeOp.js';

/** Strkeys with a detail route of their own: account, contract, pool. Balance
 *  ids (hex) and asset codes deliberately do not match — nothing links to a
 *  page that does not exist. Mirrors the JSON viewer's rule (0460 #14).
 *
 *  Checksum-verified, not shape-matched: 56 base32 characters is a shape any
 *  long base32 blob satisfies, and a link built from one points at a page
 *  that cannot exist. */
const isLinkable = isValidStrkey;

function idType(value: string): 'account' | 'contract' | 'pool' {
  if (value.startsWith('C')) return 'contract';
  if (value.startsWith('L')) return 'pool';
  return 'account';
}

/** Every linkable identifier reachable from the operation, keyed by the
 *  SHORT form the sentence prints (`GA5X…GKTM` → the full strkey).
 *
 *  Collected from the data rather than from the sentence templates: a
 *  sentence can only shorten what the API delivered, so this cannot invent
 *  an address, and the ~20 template call sites stay untouched. */
export function sentenceIds(
  light: OperationItem,
  heavy: XdrOperationDto | null
): Map<string, string> {
  const ids = new Map<string, string>();
  const visit = (value: unknown, depth = 0): void => {
    if (typeof value === 'string') {
      if (isLinkable(value)) ids.set(shortId(value), value);
      return;
    }
    if (depth > 4 || value == null || typeof value !== 'object') return;
    for (const inner of Object.values(value)) visit(inner, depth + 1);
  };
  visit(light);
  visit(detailsObj(heavy));
  return ids;
}

/** The operation's headline sentence with every identifier in it rendered as
 *  the house link + copy control, instead of dead text (0460 #11).
 *
 *  The sentence stays a plain string in `humanizeOp` — the picker and any
 *  other compact surface keep using it as text; only the presentation here
 *  splits it. */
export function HumanizedSentence({
  light,
  heavy,
  txSourceAccount,
}: {
  light: OperationItem;
  heavy: XdrOperationDto | null;
  txSourceAccount: string | null;
}) {
  const text = humanizeOp(light, heavy, txSourceAccount);
  const ids = sentenceIds(light, heavy);
  const shorts = [...ids.keys()].filter((short) => text.includes(short));
  if (shorts.length === 0) return <>{text}</>;

  // Longest first so a short form that is a prefix of another cannot win.
  const pattern = new RegExp(
    `(${shorts
      .sort((a, b) => b.length - a.length)
      .map((short) => short.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
      .join('|')})`
  );
  return (
    <>
      {text.split(pattern).map((piece, index) => {
        const full = ids.get(piece);
        return full != null ? (
          <IdentifierWithCopy
            key={index}
            value={full}
            type={idType(full)}
            tone="inherit"
            fontSize="inherit"
          />
        ) : (
          <Fragment key={index}>{piece}</Fragment>
        );
      })}
    </>
  );
}
