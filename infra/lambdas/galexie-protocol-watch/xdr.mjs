// The stellar-xdr half of the protocol watch (task 0646), kept free of I/O so
// the test can run it on plain values. `index.mjs` feeds it the major the
// running indexer was built with, Horizon's root document and the newest
// soroban-env-host releases with the stellar-xdr each is built on.
//
// The Galexie half (check.mjs) compares captive cores; this one compares the
// XDR our indexer decodes with. Protocol 29 changed no XDR and stopped only
// Galexie; a protocol that changes XDR stops the indexer even with a current
// Galexie, and only this half sees it coming. The rule is stellar-prices-
// api's `verify-xdr-protocol-gap.mjs`; the alarm only has to fire, the
// runbook takes it from there.
//
// `stellar-xdr`'s MAJOR is the protocol it decodes (28 decodes 28). Horizon
// publishes the protocol mainnet runs and the one core supports; the second
// rises weeks before the vote, which is the lead time.
//
// Our bump target is a soroban-env-host release, not stellar-xdr alone:
// contract-executor hands our XDR types to soroban-env-host, which pins one
// stellar-xdr exactly, so the two move together. On 2026-10-16 stellar-xdr
// 30.0.0 was out while the newest soroban-env-host (29.0.2) still sat on 28 —
// a bump nobody could build. A release fits protocol N when it is built on a
// stellar-xdr major above ours and NOT above N (upstream went 28 → 30, and 30
// carries protocol 30's XDR, not 29's):
//
//   OK       pin >= the protocol mainnet runs and the one core supports
//   WAITING  pin behind, but no soroban-env-host on a fitting stellar-xdr is
//            published: nothing to bump to (green — a page is for something
//            someone can act on)
//   BUMP     pin behind and a fitting release is published
//
// ponytail: compares protocol numbers; comparing the crate's XDR commit with
// core's is the exact check if this ever misfires.

/**
 * The version a crates.io dependency list requires of `name`
 * (`"=28.0.0"` → `"28.0.0"`); anything else is an error.
 */
export function requiredVersion(dependencies, name) {
  const dependency = dependencies?.dependencies?.find(
    (d) => d.crate_id === name && d.kind === 'normal'
  );
  const version = dependency?.req.match(/^[=^~]?(\d+\.\d+\.\d+)$/)?.[1];
  if (!version) {
    throw new Error(
      `cannot determine: crates.io lists no single-version ${name} requirement (${dependency?.req})`
    );
  }
  return version;
}

/** The stable, unyanked versions of a crates.io listing, newest first. */
export function stableVersions(listing) {
  if (!Array.isArray(listing?.versions)) {
    throw new Error('cannot determine: crates.io lists no versions');
  }
  return listing.versions
    .filter((v) => !v.yanked && /^\d+\.\d+\.\d+$/.test(v.num))
    .map((v) => v.num)
    .sort((a, b) => b.localeCompare(a, 'en', { numeric: true }));
}

/** The newest release of `hosts` built on a stellar-xdr in (pinned, protocol]. */
function bumpTarget(hosts, pinned, protocol) {
  return hosts.find((h) => h.xdr > pinned && h.xdr <= protocol);
}

/** The protocols from Horizon's root document; anything else is an error. */
export function networkProtocol(root) {
  const current = root.current_protocol_version;
  const supported = root.core_supported_protocol_version;
  if (!Number.isInteger(current) || !Number.isInteger(supported)) {
    throw new Error(
      `cannot determine: Horizon reports current_protocol_version=${current}, core_supported_protocol_version=${supported}`
    );
  }
  return { current, supported };
}

/**
 * `hosts` is `[{ num, xdr }]`, soroban-env-host releases newest first with
 * the stellar-xdr major each is built on; read only when the pin lags (pass
 * null otherwise). Returns `{ ok, message }`; not ok pages.
 */
export function xdrVerdict({ pinned, network, hosts }) {
  const protocol = Math.max(network.current, network.supported);
  if (pinned >= protocol) {
    return { ok: true, message: `OK: our indexer's stellar-xdr ${pinned}` };
  }
  if (!hosts?.length) {
    throw new Error('cannot determine: no soroban-env-host release was read');
  }

  const target = bumpTarget(hosts, pinned, protocol);
  if (target) {
    return {
      ok: false,
      message: `BUMP: soroban-env-host ${target.num} (stellar-xdr ${target.xdr}) is out, our indexer's stellar-xdr is ${pinned}. Runbook: docs/runbooks/galexie-protocol-watch.md`,
    };
  }
  return {
    ok: true,
    message: `WAITING: protocol ${protocol} is coming, no soroban-env-host on its stellar-xdr yet; our indexer's stellar-xdr is ${pinned}`,
  };
}
