import { describe, expect, it } from 'vitest';

import { resolveOp } from './useSelectedOp.js';

describe('resolveOp', () => {
  it('selects the first operation when the URL names none', () => {
    expect(resolveOp('', 3)).toBe(0);
  });

  it('maps the 1-based fragment onto a 0-based index', () => {
    expect(resolveOp('#op-2', 3)).toBe(1);
  });

  it('accepts the last operation', () => {
    expect(resolveOp('#op-3', 3)).toBe(2);
  });

  it('clamps a number past the end instead of letting it through', () => {
    // The regression this exists for: the index used to escape unclamped, so
    // the card rendered operation 1 while the picker beside it — handed the
    // raw 98 — highlighted nothing.
    expect(resolveOp('#op-99', 1)).toBe(0);
    expect(resolveOp('#op-99', 4)).toBe(0);
  });

  it('clamps #op-0, which is below the 1-based range', () => {
    expect(resolveOp('#op-0', 2)).toBe(0);
  });

  it('ignores a fragment that is not an operation reference', () => {
    expect(resolveOp('#summary', 3)).toBe(0);
    expect(resolveOp('#op-abc', 3)).toBe(0);
    expect(resolveOp('#op-1x', 3)).toBe(0);
  });

  it('treats an unjudgeable fragment like an absent one', () => {
    // count 0 is "still loading" and "archive fetch failed" as well as "no
    // operations" — the section renders its unavailable state for those, so
    // this must not become a claim about the fragment (0377).
    expect(resolveOp('#op-99', 0)).toBe(0);
  });
});
