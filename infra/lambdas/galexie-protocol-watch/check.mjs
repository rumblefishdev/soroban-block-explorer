// The decision of the Galexie protocol watch (task 0610), kept free of the
// AWS SDK so the test can run it against fakes. `index.mjs` feeds it our
// image from ECR and the newest image on Docker Hub.
//
// Why a newer core on Docker Hub is the whole check: captive core refuses to
// apply a ledger of a protocol it does not support. When pubnet votes, a
// Galexie whose core is one protocol short stops exporting at the vote
// ledger — P29 did that on 2026-10-01 and ingestion stood still ~14 h. SDF
// publishes the Galexie image with the new core before the vote (P27: four
// weeks, P28: four to six, P29: seven days), and an older core keeps working
// until then, so the day that image appears is the day to deploy it.

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

/**
 * OK while our core major is at least the newest Docker Hub image's; anything
 * else means a Galexie for the next protocol is out and ours is not on it.
 */
export function verdict({ ours, newest }) {
  const summary = `our Galexie core ${ours.version} | Docker Hub tag ${newest.tag} has core ${newest.version}`;

  if (ours.major >= newest.major) {
    return { ok: true, message: `OK: ${summary}` };
  }
  return {
    ok: false,
    message:
      `NEW CORE: a Galexie with core ${newest.major} is out - deploy it before the protocol ${newest.major} vote.\n` +
      `${summary}\nRunbook: docs/runbooks/galexie-protocol-watch.md`,
  };
}
