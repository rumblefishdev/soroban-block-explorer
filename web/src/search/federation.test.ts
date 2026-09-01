import { afterEach, describe, expect, it, vi } from 'vitest';

import { federatedDomain, resolveFederated } from './federation.js';

const ACCOUNT = 'GC526FUILJ6NLFXKCOOGTMDXNRW7MYSEK2UNRJV5FYWOGYDE4LOKXFEM';

function reply(body: string, ok = true) {
  return { ok, status: ok ? 200 : 404, text: () => Promise.resolve(body) };
}

/** Serve each URL from a map; anything unmapped rejects like a dead host. */
function mockFetch(routes: Record<string, ReturnType<typeof reply>>) {
  const fn = vi.fn((url: string) => {
    const hit = Object.entries(routes).find(([prefix]) =>
      url.startsWith(prefix)
    );
    return hit
      ? Promise.resolve(hit[1])
      : Promise.reject(new Error('ENOTFOUND'));
  });
  vi.stubGlobal('fetch', fn);
  return fn;
}

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
    const fetchMock = mockFetch({
      'https://lobstr.co/.well-known/stellar.toml': reply(
        'NETWORK_PASSPHRASE = "Public"\nFEDERATION_SERVER="https://lobstr.co/federation/"\n'
      ),
      'https://lobstr.co/federation/': reply(
        JSON.stringify({
          stellar_address: 'karol*lobstr.co',
          account_id: ACCOUNT,
        })
      ),
    });

    await expect(resolveFederated('karol*lobstr.co')).resolves.toEqual({
      kind: 'resolved',
      accountId: ACCOUNT,
    });

    const federationCall = fetchMock.mock.calls[1]?.[0] as string;
    expect(federationCall).toContain('type=name');
    expect(federationCall).toContain('q=karol*lobstr.co');
  });

  it('fails explicitly when the domain serves no stellar.toml', async () => {
    mockFetch({});
    const r = await resolveFederated('karol*lobstr.co');
    expect(r.kind).toBe('failed');
    expect(r.kind === 'failed' && r.reason).toContain('stellar.toml');
  });

  it('fails explicitly when the toml declares no federation server', async () => {
    mockFetch({
      'https://lobstr.co/.well-known/stellar.toml': reply(
        'VERSION = "2.0.0"\n'
      ),
    });
    const r = await resolveFederated('karol*lobstr.co');
    expect(r.kind).toBe('failed');
    expect(r.kind === 'failed' && r.reason).toContain('no federation server');
  });

  // A plaintext answer decides which account the user is sent to.
  it('refuses a non-HTTPS federation server without calling it', async () => {
    const fetchMock = mockFetch({
      'https://lobstr.co/.well-known/stellar.toml': reply(
        'FEDERATION_SERVER="http://lobstr.co/federation/"\n'
      ),
    });
    const r = await resolveFederated('karol*lobstr.co');
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
    mockFetch({
      'https://lobstr.co/.well-known/stellar.toml': reply(
        'FEDERATION_SERVER="https://lobstr.co/federation/"\n'
      ),
      'https://lobstr.co/federation/': reply(body),
    });
    const r = await resolveFederated('karol*lobstr.co');
    expect(r.kind).toBe('failed');
  });

  it('fails explicitly when the federation server 404s the name', async () => {
    mockFetch({
      'https://lobstr.co/.well-known/stellar.toml': reply(
        'FEDERATION_SERVER="https://lobstr.co/federation/"\n'
      ),
      'https://lobstr.co/federation/': reply('{"detail":"not found"}', false),
    });
    const r = await resolveFederated('nobody*lobstr.co');
    expect(r.kind).toBe('failed');
    expect(r.kind === 'failed' && r.reason).toContain('did not resolve');
  });
});
