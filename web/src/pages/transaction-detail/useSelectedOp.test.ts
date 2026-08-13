import { describe, expect, it } from 'vitest';

import { resolveOp } from './useSelectedOp.js';

describe('resolveOp', () => {
  it('selects the first operation when the URL names none', () => {
    expect(resolveOp('', 3)).toEqual({ index: 0, missing: null });
  });

  it('maps the 1-based fragment onto a 0-based index', () => {
    expect(resolveOp('#op-2', 3)).toEqual({ index: 1, missing: null });
  });

  it('accepts the last operation', () => {
    expect(resolveOp('#op-3', 3)).toEqual({ index: 2, missing: null });
  });

  it('reports a number past the end instead of hiding the operation', () => {
    // The regression this task exists for: the section used to render a
    // message INSTEAD of the operation, which blanked the page for the
    // single-operation transactions that make up ~85 % of mainnet traffic.
    expect(resolveOp('#op-99', 1)).toEqual({ index: 0, missing: 99 });
  });

  it('reports #op-0 rather than silently treating it as the first', () => {
    // `Math.max(0, N - 1)` used to fold 0 into operation 1 with nothing said.
    expect(resolveOp('#op-0', 2)).toEqual({ index: 0, missing: 0 });
  });

  it('ignores a fragment that is not an operation reference', () => {
    expect(resolveOp('#summary', 3)).toEqual({ index: 0, missing: null });
    expect(resolveOp('#op-abc', 3)).toEqual({ index: 0, missing: null });
    expect(resolveOp('#op-1x', 3)).toEqual({ index: 0, missing: null });
  });

  it('claims nothing while the list is unknown', () => {
    // count 0 is "still loading" and "archive fetch failed" as well as "no
    // operations" — answering it would assert a count nobody measured (0377).
    expect(resolveOp('#op-99', 0)).toEqual({ index: 0, missing: null });
  });
});
