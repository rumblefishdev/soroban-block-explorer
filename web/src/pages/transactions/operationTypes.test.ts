import { describe, expect, it } from 'vitest';

import {
  formatOperationType,
  normalizeOperationType,
  OPERATION_TYPE_OPTIONS,
} from './operationTypes.js';

describe('normalizeOperationType', () => {
  it('uppercases a valid lowercase op type', () => {
    expect(normalizeOperationType('payment')).toBe('PAYMENT');
    expect(normalizeOperationType('invoke_host_function')).toBe(
      'INVOKE_HOST_FUNCTION'
    );
  });

  it('passes valid SCREAMING_SNAKE_CASE through', () => {
    expect(normalizeOperationType('PAYMENT')).toBe('PAYMENT');
  });

  it('trims surrounding whitespace', () => {
    expect(normalizeOperationType('  payment  ')).toBe('PAYMENT');
  });

  it('collapses unknown values to "" so the API never sees them', () => {
    expect(normalizeOperationType('not-a-real-op')).toBe('');
    expect(normalizeOperationType('p4ym3nt')).toBe('');
  });

  it('returns "" for nullish input', () => {
    expect(normalizeOperationType(null)).toBe('');
    expect(normalizeOperationType(undefined)).toBe('');
    expect(normalizeOperationType('')).toBe('');
  });
});

describe('formatOperationType', () => {
  it('uses the friendly label when one is registered', () => {
    expect(formatOperationType('PAYMENT')).toBe('Payment');
    // Soroban op uses a friendlier override than the title-cased default.
    expect(formatOperationType('INVOKE_HOST_FUNCTION')).toBe('Invoke Contract');
  });

  it('falls back to title-casing the SCREAMING_SNAKE_CASE for unknown ops', () => {
    // Unknown but well-formed → "Unknown Future Op".
    expect(formatOperationType('UNKNOWN_FUTURE_OP')).toBe('Unknown Future Op');
  });
});

describe('OPERATION_TYPE_OPTIONS', () => {
  it('exposes the Protocol 21 op set in XDR discriminant order', () => {
    const values = OPERATION_TYPE_OPTIONS.map((o) => o.value);
    // Sanity check on a few well-known anchor entries.
    expect(values[0]).toBe('CREATE_ACCOUNT');
    expect(values[1]).toBe('PAYMENT');
    expect(values).toContain('INVOKE_HOST_FUNCTION');
  });

  it('every option has a non-empty label and SCREAMING_SNAKE value', () => {
    for (const opt of OPERATION_TYPE_OPTIONS) {
      expect(opt.value).toMatch(/^[A-Z][A-Z_]+$/);
      expect(opt.label.length).toBeGreaterThan(0);
    }
  });
});
