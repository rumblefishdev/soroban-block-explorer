import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  federatedDomain,
  resolveFederated,
  resolveFederatedName,
} from './federation.js';

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

describe('resolveFederatedName (reverse, type=id)', () => {
  const TOML = 'https://lobstr.co/.well-known/stellar.toml';
  const SERVER = 'https://lobstr.co/federation/';
  const withServer = {
    [TOML]: reply('FEDERATION_SERVER="https://lobstr.co/federation/"\n'),
  };

  it('returns the address the domain claims for the account', async () => {
    const fetchMock = mockFetch({
      ...withServer,
      [SERVER]: reply(JSON.stringify({ stellar_address: 'karol*lobstr.co' })),
    });

    await expect(resolveFederatedName(ACCOUNT, 'lobstr.co')).resolves.toBe(
      'karol*lobstr.co'
    );
    expect(fetchMock.mock.calls[1]?.[0] as string).toContain('type=id');
  });

  // The account named this domain; the domain must name the account back
  // inside its OWN namespace, or it is claiming it into someone else's.
  it('rejects an address that does not live at the account home domain', async () => {
    mockFetch({
      ...withServer,
      [SERVER]: reply(
        JSON.stringify({ stellar_address: 'karol*evil.example' })
      ),
    });

    await expect(
      resolveFederatedName(ACCOUNT, 'lobstr.co')
    ).resolves.toBeNull();
  });

  it('is silent when the domain publishes no federation server', async () => {
    mockFetch({ [TOML]: reply('VERSION = "2.0.0"\n') });
    await expect(
      resolveFederatedName(ACCOUNT, 'lobstr.co')
    ).resolves.toBeNull();
  });

  it('is silent when the account has no registered name', async () => {
    mockFetch({
      ...withServer,
      [SERVER]: reply(JSON.stringify({ detail: 'not found' })),
    });
    await expect(
      resolveFederatedName(ACCOUNT, 'lobstr.co')
    ).resolves.toBeNull();
  });

  it.each([
    ['an empty home domain', ACCOUNT, ''],
    ['a non-account id', 'not-a-key', 'lobstr.co'],
  ])('makes no request for %s', async (_label, id, domain) => {
    const fetchMock = mockFetch({});
    await expect(resolveFederatedName(id, domain)).resolves.toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
