import { describe, expect, it } from 'vitest';

import {
  networkProtocol,
  requiredVersion,
  stableVersions,
  xdrVerdict,
} from '../xdr.mjs';

// The tier matrix of stellar-prices-api's verify-xdr-protocol-gap test, with
// our bump target: a soroban-env-host release and the stellar-xdr major it is
// built on. `p` is our pin; every case is relative to it.
const p = 28;
const net = (current, supported) => ({ current, supported });
const host = (num, xdr) => ({ num, xdr });

// [name, network, hosts (newest first), ok, text]
// prettier-ignore
const cases = [
  ['level', net(p, p), null, true, `OK: our indexer's stellar-xdr ${p} | mainnet protocol ${p} | core supports ${p}`],
  ['ahead of core', net(p - 1, p - 1), null, true, 'OK:'],
  ['WAITING before the vote', net(p, p + 1), [host('29.0.2', p)], true, `WAITING: no soroban-env-host on a newer stellar-xdr, up to protocol ${p + 1}, is published yet`],
  ['LAGGING', net(p, p + 1), [host('30.0.0', p + 1), host('29.0.2', p)], false, `LAGGING: core supports protocol ${p + 1} and soroban-env-host 30.0.0 on stellar-xdr ${p + 1} is published`],
  ['voted, no fitting host (P29)', net(p + 1, p + 1), [host('29.0.2', p)], true, 'WAITING:'],
  ['BEHIND', net(p + 1, p + 1), [host('30.0.0', p + 1), host('29.0.2', p)], false, `BEHIND: mainnet runs protocol ${p + 1} and soroban-env-host 30.0.0 on stellar-xdr ${p + 1} is published`],
  ['BEHIND, host for mainnet but not core', net(p + 1, p + 2), [host('30.0.0', p + 1)], false, 'BEHIND:'],
  ['voted, only a host on a later xdr', net(p + 1, p + 1), [host('31.0.0', p + 2), host('29.0.2', p)], true, 'WAITING:'],
  ['before the vote, only a host on a later xdr', net(p, p + 1), [host('31.0.0', p + 2), host('29.0.2', p)], true, 'WAITING:'],
  ['voted without a host, core next with one (P29 → P30)', net(p + 1, p + 2), [host('31.0.0', p + 2), host('29.0.2', p)], false, `LAGGING: core supports protocol ${p + 2} and soroban-env-host 31.0.0 on stellar-xdr ${p + 2}`],
  ['the newest fitting host, past a later one', net(p, p + 1), [host('31.0.0', p + 2), host('30.0.1', p + 1), host('30.0.0', p + 1)], false, 'soroban-env-host 30.0.1 on stellar-xdr'],
  // 2026-10-16: core 30, stellar-xdr 30.0.0 out, soroban-env-host 29.0.2 on 28.
  ['today', net(p + 1, p + 2), [host('29.0.2', p)], true, `WAITING: no soroban-env-host on a newer stellar-xdr, up to protocol ${p + 2}, is published yet - nothing to bump to. our indexer's stellar-xdr ${p} | mainnet protocol ${p + 1} | core supports ${p + 2} | newest soroban-env-host 29.0.2 (stellar-xdr ${p})`],
];

describe('xdrVerdict', () => {
  it.each(cases)('%s', (_name, network, hosts, ok, text) => {
    const result = xdrVerdict({ pinned: p, network, hosts });
    expect(result.ok).toBe(ok);
    expect(result.message).toContain(text);
  });

  it('writes the whole page: verdict, readings, runbook', () => {
    expect(
      xdrVerdict({
        pinned: p,
        network: net(p + 1, p + 1),
        hosts: [host('30.0.0', p + 1), host('29.0.2', p)],
      }).message
    ).toBe(
      `BEHIND: mainnet runs protocol ${
        p + 1
      } and soroban-env-host 30.0.0 on stellar-xdr ${
        p + 1
      } is published - the indexer may stop at the next ledger. Bump both and deploy now.\n` +
        `our indexer's stellar-xdr ${p} | mainnet protocol ${
          p + 1
        } | core supports ${
          p + 1
        } | newest soroban-env-host 30.0.0 (stellar-xdr ${p + 1})\n` +
        'Runbook: docs/runbooks/galexie-protocol-watch.md'
    );
    expect(
      xdrVerdict({
        pinned: p,
        network: net(p, p + 1),
        hosts: [host('30.0.0', p + 1)],
      }).message
    ).toBe(
      `LAGGING: core supports protocol ${
        p + 1
      } and soroban-env-host 30.0.0 on stellar-xdr ${
        p + 1
      } is published - bump both before the vote, testnet first.\n` +
        `our indexer's stellar-xdr ${p} | mainnet protocol ${p} | core supports ${
          p + 1
        } | newest soroban-env-host 30.0.0 (stellar-xdr ${p + 1})\n` +
        'Runbook: docs/runbooks/galexie-protocol-watch.md'
    );
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
