import { describe, expect, it } from 'vitest';

import {
  networkProtocol,
  requiredVersion,
  stableVersions,
  xdrVerdict,
} from '../xdr.mjs';

// The cases of stellar-prices-api's verify-xdr-protocol-gap test, with our
// bump target: a soroban-env-host release and the stellar-xdr major it is
// built on. `p` is our pin; every case is relative to it.
const p = 28;
const net = (current, supported) => ({ current, supported });
const host = (num, xdr) => ({ num, xdr });
const bump = (num, xdr) =>
  `BUMP: soroban-env-host ${num} (stellar-xdr ${xdr}) is out, our indexer's stellar-xdr is ${p}. Runbook: docs/runbooks/galexie-protocol-watch.md`;

// [name, network, hosts (newest first), ok, whole message]
// prettier-ignore
const cases = [
  ['level', net(p, p), null, true, `OK: our indexer's stellar-xdr ${p}`],
  ['ahead of core', net(p - 1, p - 1), null, true, `OK: our indexer's stellar-xdr ${p}`],
  ['before the vote, no fitting host', net(p, p + 1), [host('29.0.2', p)], true, `WAITING: protocol ${p + 1} is coming, no soroban-env-host on its stellar-xdr yet; our indexer's stellar-xdr is ${p}`],
  ['before the vote, host published', net(p, p + 1), [host('30.0.0', p + 1), host('29.0.2', p)], false, bump('30.0.0', p + 1)],
  ['voted, no fitting host (P29)', net(p + 1, p + 1), [host('29.0.2', p)], true, `WAITING: protocol ${p + 1} is coming, no soroban-env-host on its stellar-xdr yet; our indexer's stellar-xdr is ${p}`],
  ['voted, host published', net(p + 1, p + 1), [host('30.0.0', p + 1), host('29.0.2', p)], false, bump('30.0.0', p + 1)],
  ['host for mainnet but not for core', net(p + 1, p + 2), [host('30.0.0', p + 1)], false, bump('30.0.0', p + 1)],
  ['voted, only a host on a later xdr', net(p + 1, p + 1), [host('31.0.0', p + 2), host('29.0.2', p)], true, `WAITING: protocol ${p + 1} is coming, no soroban-env-host on its stellar-xdr yet; our indexer's stellar-xdr is ${p}`],
  ['before the vote, only a host on a later xdr', net(p, p + 1), [host('31.0.0', p + 2), host('29.0.2', p)], true, `WAITING: protocol ${p + 1} is coming, no soroban-env-host on its stellar-xdr yet; our indexer's stellar-xdr is ${p}`],
  ['voted without a host, core next with one (P29 → P30)', net(p + 1, p + 2), [host('31.0.0', p + 2), host('29.0.2', p)], false, bump('31.0.0', p + 2)],
  ['the newest fitting host, past a later one', net(p, p + 1), [host('31.0.0', p + 2), host('30.0.1', p + 1), host('30.0.0', p + 1)], false, bump('30.0.1', p + 1)],
  ['core reports less than mainnet runs', net(p + 1, p), [host('30.0.0', p + 1)], false, bump('30.0.0', p + 1)],
  // 2026-10-16 22:00: core 30, stellar-xdr 30.0.0 out, soroban-env-host 29.0.2 on 28.
  ['16 October', net(p + 1, p + 2), [host('29.0.2', p)], true, `WAITING: protocol ${p + 2} is coming, no soroban-env-host on its stellar-xdr yet; our indexer's stellar-xdr is ${p}`],
  // 2026-10-19: soroban-env-host 30.0.0 on stellar-xdr 30 published.
  ['19 October', net(p + 1, p + 2), [host('30.0.0', p + 2), host('29.0.2', p)], false, bump('30.0.0', p + 2)],
];

describe('xdrVerdict', () => {
  it.each(cases)('%s', (_name, network, hosts, ok, message) => {
    expect(xdrVerdict({ pinned: p, network, hosts })).toEqual({ ok, message });
  });

  it('throws when the pin lags and no host was read', () => {
    for (const hosts of [null, []]) {
      expect(() =>
        xdrVerdict({ pinned: p, network: net(p, p + 1), hosts })
      ).toThrow('cannot determine: no soroban-env-host release was read');
    }
  });
});

describe('requiredVersion', () => {
  const deps = (...list) => ({ dependencies: list });
  const dep = (crate_id, req, kind = 'normal') => ({ crate_id, req, kind });

  it('reads an exact requirement, as soroban-env-host 29.0.2 lists it', () => {
    expect(
      requiredVersion(
        deps(
          dep('soroban-env-macros', '=29.0.2', 'dev'),
          dep('soroban-env-common', '=29.0.2')
        ),
        'soroban-env-common'
      )
    ).toBe('29.0.2');
    expect(
      requiredVersion(deps(dep('stellar-xdr', '28.0.0')), 'stellar-xdr')
    ).toBe('28.0.0');
  });

  it('finds its crate among the others and reads multi-digit parts', () => {
    expect(
      requiredVersion(
        deps(dep('serde', '=1.0.0'), dep('stellar-xdr', '=28.10.0')),
        'stellar-xdr'
      )
    ).toBe('28.10.0');
    expect(
      requiredVersion(deps(dep('stellar-xdr', '=28.0.10')), 'stellar-xdr')
    ).toBe('28.0.10');
  });

  it('ignores a dev dependency of the same name', () => {
    expect(() =>
      requiredVersion(deps(dep('stellar-xdr', '=30.0.0', 'dev')), 'stellar-xdr')
    ).toThrow(
      'cannot determine: crates.io lists no single-version stellar-xdr'
    );
  });

  it.each([
    ['missing', deps(dep('serde', '1'))],
    ['a range', deps(dep('stellar-xdr', '>=28, <30'))],
    ['a bare major', deps(dep('stellar-xdr', '28'))],
    ['not a dependency list', {}],
    ['no document', undefined],
    ['a pre-release', deps(dep('stellar-xdr', '=30.0.0-rc.1'))],
    ['a prefixed requirement', deps(dep('stellar-xdr', '<=28.0.0'))],
  ])('throws on %s', (_name, list) => {
    expect(() => requiredVersion(list, 'stellar-xdr')).toThrow(
      /^cannot determine: crates.io lists no single-version stellar-xdr/
    );
  });
});

describe('stableVersions', () => {
  it('drops yanked and pre-releases and sorts newest first', () => {
    // The order crates.io served soroban-env-host on 2026-10-16, plus two
    // made-up versions for the pre-release and the two-digit patch.
    const listing = {
      versions: [
        { num: '29.0.2', yanked: false },
        { num: '29.0.1', yanked: false },
        { num: '29.0.0', yanked: true },
        { num: '28.0.2', yanked: false },
        { num: '25.2.2', yanked: false },
        { num: '26.1.4', yanked: false },
        { num: '30.0.0-rc.1', yanked: false },
        { num: '29.0.10', yanked: false },
        { num: '29.10.0', yanked: false },
      ],
    };
    expect(stableVersions(listing)).toEqual([
      '29.10.0',
      '29.0.10',
      '29.0.2',
      '29.0.1',
      '28.0.2',
      '26.1.4',
      '25.2.2',
    ]);
  });

  it('throws on a listing without versions', () => {
    expect(() => stableVersions(undefined)).toThrow(
      'cannot determine: crates.io lists no versions'
    );
    expect(() => stableVersions({})).toThrow(
      'cannot determine: crates.io lists no versions'
    );
  });
});

describe('networkProtocol', () => {
  it('reads both protocols', () => {
    expect(
      networkProtocol({
        current_protocol_version: 29,
        core_supported_protocol_version: 30,
      })
    ).toEqual({ current: 29, supported: 30 });
  });

  it('throws when either is missing', () => {
    expect(() => networkProtocol({ current_protocol_version: 29 })).toThrow(
      /^cannot determine: Horizon reports/
    );
    expect(() =>
      networkProtocol({ core_supported_protocol_version: 30 })
    ).toThrow(/^cannot determine: Horizon reports/);
  });
});
