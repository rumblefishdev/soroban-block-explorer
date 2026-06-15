import { describe, expect, it } from 'vitest';

import { formatReturnType } from './interfaceMetadata.js';

describe('formatReturnType', () => {
  it('returns "void" for an empty outputs array', () => {
    expect(formatReturnType([])).toBe('void');
  });

  it('returns the single output as-is', () => {
    expect(formatReturnType(['bool'])).toBe('bool');
    expect(formatReturnType(['i128'])).toBe('i128');
  });

  it('joins multiple outputs with commas', () => {
    expect(formatReturnType(['bool', 'i128'])).toBe('bool, i128');
  });
});
