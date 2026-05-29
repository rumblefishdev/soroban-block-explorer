import { describe, expect, it } from 'vitest';

import {
  formatReturnType,
  parseInterfaceMetadata,
} from './interfaceMetadata.js';

describe('parseInterfaceMetadata', () => {
  it('returns null for null / undefined input (SAC / pre-upload contracts)', () => {
    expect(parseInterfaceMetadata(null)).toBeNull();
    expect(parseInterfaceMetadata(undefined)).toBeNull();
  });

  it('returns null when the raw blob is not an object', () => {
    expect(parseInterfaceMetadata('not an object')).toBeNull();
    expect(parseInterfaceMetadata(42)).toBeNull();
    // Arrays look like objects but are rejected explicitly.
    expect(parseInterfaceMetadata([])).toBeNull();
  });

  it('returns null when functions is missing or not an array', () => {
    expect(parseInterfaceMetadata({})).toBeNull();
    expect(parseInterfaceMetadata({ functions: 'oops' })).toBeNull();
    expect(parseInterfaceMetadata({ functions: {} })).toBeNull();
  });

  it('parses a well-formed function spec', () => {
    const parsed = parseInterfaceMetadata({
      functions: [
        {
          name: 'transfer',
          doc: 'Move tokens.',
          inputs: [
            { name: 'from', type_name: 'Address' },
            { name: 'to', type_name: 'Address' },
            { name: 'amount', type_name: 'i128' },
          ],
          outputs: ['bool'],
        },
      ],
      wasm_byte_len: 1024,
    });

    expect(parsed).toEqual({
      functions: [
        {
          name: 'transfer',
          doc: 'Move tokens.',
          inputs: [
            { name: 'from', type_name: 'Address' },
            { name: 'to', type_name: 'Address' },
            { name: 'amount', type_name: 'i128' },
          ],
          outputs: ['bool'],
        },
      ],
      wasm_byte_len: 1024,
    });
  });

  it('drops unnamed functions and inputs (the render code keys by name)', () => {
    const parsed = parseInterfaceMetadata({
      functions: [
        {
          // unnamed function — dropped
          inputs: [],
          outputs: [],
        },
        {
          name: 'balance',
          inputs: [
            { name: 'id', type_name: 'Address' },
            { name: '', type_name: 'i128' }, // unnamed input — dropped
          ],
          outputs: ['i128'],
        },
      ],
    });

    expect(parsed?.functions).toHaveLength(1);
    expect(parsed?.functions[0].name).toBe('balance');
    expect(parsed?.functions[0].inputs).toEqual([
      { name: 'id', type_name: 'Address' },
    ]);
  });

  it('falls back to safe defaults when sub-fields have the wrong types', () => {
    const parsed = parseInterfaceMetadata({
      functions: [
        {
          name: 'mint',
          // doc / inputs / outputs all wrong types — coerced to defaults.
          doc: 42,
          inputs: 'not an array',
          outputs: { 0: 'i128' },
        },
      ],
      // missing wasm_byte_len → defaults to 0
    });

    expect(parsed?.functions[0]).toEqual({
      name: 'mint',
      doc: '',
      inputs: [],
      outputs: [],
    });
    expect(parsed?.wasm_byte_len).toBe(0);
  });

  it('coerces unknown input type_name to "unknown" without throwing', () => {
    const parsed = parseInterfaceMetadata({
      functions: [
        {
          name: 'transfer',
          inputs: [{ name: 'from', type_name: 12345 /* wrong type */ }],
          outputs: [],
        },
      ],
    });

    expect(parsed?.functions[0].inputs[0]).toEqual({
      name: 'from',
      type_name: 'unknown',
    });
  });
});

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
