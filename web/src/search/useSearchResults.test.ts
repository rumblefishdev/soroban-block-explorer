import { describe, expect, it } from 'vitest';

import { federatedDomain } from './federation.js';

describe('useSearchResults — federated queries never reach /v1/search', () => {
  // The suppression lives in the hook, not at its callers: both call sites
  // needed the same rule, and a third search bar that forgot it would print
  // "No results for karol*lobstr.co" — a claim that the address does not
  // exist, while the results page is about to resolve it (task 0443).
  //
  // The hook computes `enabled` as `effectiveQuery.length > 0 &&
  // federatedDomain(effectiveQuery) == null`, so the classifier below is the
  // whole of the rule; these cases pin which inputs it takes out of the
  // buckets and, more importantly, which it leaves in.
  it.each([
    ['karol*lobstr.co', 'lobstr.co'],
    ['KAROL*LOBSTR.CO', 'lobstr.co'],
  ])('suppresses %s', (q, domain) => {
    expect(federatedDomain(q)).toBe(domain);
  });

  it.each([
    ['kale'],
    ['GC526FUILJ6NLFXKCOOGTMDXNRW7MYSEK2UNRJV5FYWOGYDE4LOKXFEM'],
    ['not*a-domain'],
    ['a*b*c.co'],
  ])('leaves %s to the buckets', (q) => {
    expect(federatedDomain(q)).toBeNull();
  });
});
