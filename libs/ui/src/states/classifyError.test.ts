import { describe, expect, it } from 'vitest';

import { classifyError, isMissingResource } from './classifyError.js';

describe('classifyError', () => {
  it('returns "unknown" for null / undefined', () => {
    expect(classifyError(null)).toBe('unknown');
    expect(classifyError(undefined)).toBe('unknown');
  });

  it('maps known HTTP statuses on objects with a numeric `status` field', () => {
    expect(classifyError({ status: 404 })).toBe('not-found');
    expect(classifyError({ status: 429 })).toBe('rate-limit');
    expect(classifyError({ status: 500 })).toBe('transient');
    expect(classifyError({ status: 503 })).toBe('transient');
    expect(classifyError({ status: 400 })).toBe('validation');
    expect(classifyError({ status: 422 })).toBe('validation');
  });

  it('returns "unknown" for status codes that are not classified', () => {
    expect(classifyError({ status: 401 })).toBe('unknown');
    expect(classifyError({ status: 418 })).toBe('unknown');
  });

  it('reads `status` off a real `Response` object', () => {
    expect(classifyError(new Response(null, { status: 404 }))).toBe(
      'not-found'
    );
  });

  it('treats a native `TypeError` as transient (network / fetch error)', () => {
    expect(classifyError(new TypeError('fetch failed'))).toBe('transient');
  });

  it('returns "unknown" for any other Error or non-status object', () => {
    expect(classifyError(new Error('boom'))).toBe('unknown');
    expect(classifyError({})).toBe('unknown');
    expect(classifyError({ status: 'not-a-number' })).toBe('unknown');
  });
});

describe('isMissingResource', () => {
  it('returns true for not-found and validation', () => {
    expect(isMissingResource('not-found')).toBe(true);
    expect(isMissingResource('validation')).toBe(true);
  });

  it('returns false for transient / rate-limit / unknown', () => {
    expect(isMissingResource('transient')).toBe(false);
    expect(isMissingResource('rate-limit')).toBe(false);
    expect(isMissingResource('unknown')).toBe(false);
  });
});
