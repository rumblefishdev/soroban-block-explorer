import { afterEach, describe, expect, it, vi } from 'vitest';

import { fetchReply, stubFetch } from '../test-utils.js';

import { federatedDomain, resolveFederated } from './federation.js';

const ACCOUNT = 'GC526FUILJ6NLFXKCOOGTMDXNRW7MYSEK2UNRJV5FYWOGYDE4LOKXFEM';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('federatedDomain', () => {
  it('accepts a federated address and lowercases the domain', () => {
    expect(federatedDomain('karol*LOBSTR.co')).toBe('lobstr.co');
    expect(federatedDomain('  a*sub.example.org  ')).toBe('sub.example.org');
  });

  // The classifier is the gate on spending a network round-trip, so every
  // near-miss below must stay a miss.
  it.each([
    ['a plain account id', ACCOUNT],
    ['no star', 'karol.lobstr.co'],
    ['two stars', 'a*b*c.co'],
    ['no domain dot', 'karol*localhost'],
    ['hyphenated but dotless domain', 'not*a-domain'],
    ['numeric tld', 'karol*example.12'],
    ['whitespace inside', 'kar ol*lobstr.co'],
    ['empty name', '*lobstr.co'],
    ['empty', ''],
  ])('rejects %s', (_label, input) => {
    expect(federatedDomain(input)).toBeNull();
  });
});

describe('resolveFederated', () => {
  it('resolves through stellar.toml to the account id', async () => {
    const fetchMock = stubFetch({
      'https://lobstr.co/.well-known/stellar.toml': fetchReply(
        'NETWORK_PASSPHRASE = "Public"\nFEDERATION_SERVER="https://lobstr.co/federation/"\n'
      ),
      'https://lobstr.co/federation/': fetchReply(
        JSON.stringify({
          stellar_address: 'karol*lobstr.co',
          account_id: ACCOUNT,
        })
      ),
    });

    await expect(
      resolveFederated('karol*lobstr.co', 'lobstr.co')
    ).resolves.toEqual({
      kind: 'resolved',
      accountId: ACCOUNT,
    });

    const federationCall = fetchMock.mock.calls[1]?.[0] as string;
    expect(federationCall).toContain('type=name');
    expect(federationCall).toContain('q=karol*lobstr.co');
  });

  // The domain owner learns the viewer's IP by virtue of being fetched. It
  // must not also learn which page they came from, and none of our cookies
  // may travel with the request.
  it('sends no referrer and no credentials', async () => {
    const fetchMock = stubFetch({
      'https://lobstr.co/.well-known/stellar.toml': fetchReply(
        'FEDERATION_SERVER="https://lobstr.co/federation/"\n'
      ),
      'https://lobstr.co/federation/': fetchReply(
        JSON.stringify({ account_id: ACCOUNT })
      ),
    });

    await resolveFederated('karol*lobstr.co', 'lobstr.co');

    for (const call of fetchMock.mock.calls) {
      const init = (call as unknown as [string, RequestInit])[1];
      expect(init.referrerPolicy).toBe('no-referrer');
      expect(init.credentials).toBe('omit');
    }
  });

  it('fails explicitly when the domain serves no stellar.toml', async () => {
    stubFetch({});
    const r = await resolveFederated('karol*lobstr.co', 'lobstr.co');
    expect(r.kind).toBe('failed');
    expect(r.kind === 'failed' && r.reason).toContain('stellar.toml');
  });

  it('fails explicitly when the toml declares no federation server', async () => {
    stubFetch({
      'https://lobstr.co/.well-known/stellar.toml': fetchReply(
        'VERSION = "2.0.0"\n'
      ),
    });
    const r = await resolveFederated('karol*lobstr.co', 'lobstr.co');
    expect(r.kind).toBe('failed');
    expect(r.kind === 'failed' && r.reason).toContain('no federation server');
  });

  // A plaintext answer decides which account the user is sent to.
  it('refuses a non-HTTPS federation server without calling it', async () => {
    const fetchMock = stubFetch({
      'https://lobstr.co/.well-known/stellar.toml': fetchReply(
        'FEDERATION_SERVER="http://lobstr.co/federation/"\n'
      ),
    });
    const r = await resolveFederated('karol*lobstr.co', 'lobstr.co');
    expect(r.kind).toBe('failed');
    expect(r.kind === 'failed' && r.reason).toContain('non-HTTPS');
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  // The server is run by the domain owner; its answer becomes a route.
  it.each([
    ['a non-account string', JSON.stringify({ account_id: 'not-a-key' })],
    ['a contract id', JSON.stringify({ account_id: `C${ACCOUNT.slice(1)}` })],
    ['no account_id', JSON.stringify({ detail: 'not found' })],
    ['unparseable json', '<html>404</html>'],
  ])('rejects %s from the federation server', async (_label, body) => {
    stubFetch({
      'https://lobstr.co/.well-known/stellar.toml': fetchReply(
        'FEDERATION_SERVER="https://lobstr.co/federation/"\n'
      ),
      'https://lobstr.co/federation/': fetchReply(body),
    });
    const r = await resolveFederated('karol*lobstr.co', 'lobstr.co');
    expect(r.kind).toBe('failed');
  });

  it('fails explicitly when the federation server 404s the name', async () => {
    stubFetch({
      'https://lobstr.co/.well-known/stellar.toml': fetchReply(
        'FEDERATION_SERVER="https://lobstr.co/federation/"\n'
      ),
      'https://lobstr.co/federation/': fetchReply(
        '{"detail":"not found"}',
        false
      ),
    });
    const r = await resolveFederated('nobody*lobstr.co', 'lobstr.co');
    expect(r.kind).toBe('failed');
    expect(r.kind === 'failed' && r.reason).toContain('did not resolve');
  });
});
