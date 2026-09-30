import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import { Fragment } from 'react';
import {
  addressType,
  IdentifierWithCopy,
} from '@rumblefish/soroban-block-explorer-ui';

import { detailsObj, humanizeOp, shortId } from './humanizeOp.js';

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
      // Only an address with a page of its own: balance ids (hex) and asset
      // codes never link, like in the JSON viewer (0460 #14).
      if (addressType(value) != null) ids.set(shortId(value), value);
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
        const type = full != null ? addressType(full) : null;
        return full != null && type != null ? (
          <IdentifierWithCopy
            key={index}
            value={full}
            type={type}
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
