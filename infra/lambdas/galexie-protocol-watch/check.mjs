// The decision of the Galexie protocol watch (task 0610), kept free of the
// AWS SDK so the test can run it against fakes. `index.mjs` feeds it the real
// Horizon, ECR and Docker Hub.
//
// Why the protocol number is the whole check: captive core refuses to apply a
// ledger of a protocol it does not support. When pubnet votes, a Galexie
// whose core is one protocol short stops exporting at the vote ledger — P29
// did that on 2026-10-01 and ingestion stood still ~14 h. Horizon reports
// `core_supported_protocol_version` days before the vote (P29: from about
// 2026-09-22), so "our core < what the network's core supports" is the early
// warning, and "our core < what the network runs" means the stall is here.

/**
 * The captive-core version baked into an image, read from its config's
 * `STELLAR_CORE_VERSION`. Not from tags: our pin is the ECR digest, which is
 * not on Docker Hub for every release (26.1.0 and 27.0.0 are not), and an
 * image's `org.opencontainers.image.version` label is the Ubuntu base.
 *
 * `registry` reads one repository: `manifest(tagOrDigest)` and
 * `blob(digest)`, both returning parsed JSON.
 */
export async function coreVersion(registry, reference) {
  let manifest = await registry.manifest(reference);

  // A manifest list: Galexie runs on linux/amd64 (ingestion-stack.ts).
  if (manifest.manifests) {
    const amd64 = manifest.manifests.find(
      (m) => m.platform?.os === 'linux' && m.platform?.architecture === 'amd64'
    );
    if (!amd64) {
      throw new Error(
        `cannot determine: ${reference} has no linux/amd64 image`
      );
    }
    manifest = await registry.manifest(amd64.digest);
  }

  const config = await registry.blob(manifest.config.digest);
  const env = config.config?.Env ?? [];
  const entry = env.find((line) => line.startsWith('STELLAR_CORE_VERSION='));
  if (!entry) {
    throw new Error(
      `cannot determine: ${reference} carries no STELLAR_CORE_VERSION`
    );
  }

  const version = entry.slice('STELLAR_CORE_VERSION='.length);
  const major = Number(version.split('.')[0]);
  if (!Number.isInteger(major)) {
    throw new Error(`cannot determine: STELLAR_CORE_VERSION=${version}`);
  }
  return { version, major };
}

/** Our core applies both what the network runs and what it is ready to vote. */
export function coreIsReady({ network, ours }) {
  return ours.major >= network.supported && ours.major >= network.current;
}

/**
 * OK, LAGGING (the vote is ahead, bump now) or BEHIND (the network already
 * runs a protocol our core cannot apply). `newest` is the newest image on
 * Docker Hub, or `{ error }` when Hub could not be read; it is read only when
 * the core is not ready, and says whether the bump is possible today. A Hub
 * outage alone never fails the check.
 */
export function verdict({ network, ours, newest }) {
  const summary =
    `our Galexie core ${ours.version} | the network runs protocol ${network.current}` +
    ` | its core supports ${network.supported}`;

  if (coreIsReady({ network, ours })) {
    return { ok: true, message: `OK: ${summary}` };
  }

  const needed = Math.max(network.supported, network.current);
  const tier =
    ours.major < network.current
      ? `BEHIND: the network already runs protocol ${network.current} - Galexie cannot apply its ledgers.`
      : `LAGGING: the network core supports protocol ${network.supported} before the vote - bump Galexie now.`;

  let hub;
  if (newest.error) {
    hub = `Docker Hub could not be read: ${newest.error}`;
  } else if (newest.major >= needed) {
    hub = `Docker Hub tag ${newest.tag} has core ${newest.version} - the bump is possible today.`;
  } else {
    hub = `Docker Hub's newest tag ${newest.tag} has core ${newest.version} - no image for protocol ${needed} yet.`;
  }

  return {
    ok: false,
    message: `${tier}\n${summary}\n${hub}\nRunbook: docs/runbooks/galexie-protocol-watch.md`,
  };
}
