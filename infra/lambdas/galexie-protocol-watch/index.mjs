// Protocol watch, every 3 hours, two checks:
//   - Galexie (task 0610): the captive-core version of the Galexie that runs
//     against the newest Galexie on Docker Hub;
//   - stellar-xdr (task 0646): the major the running indexer was built with
//     against the protocol Horizon says core supports, and crates.io.
// Either one finding something to do — or failing to read — makes the run
// throw, and the alarm on this function's Errors goes to Slack. Both reasons
// are in the log.
//
// The AWS SDK v3 comes with the Lambda Node.js runtime; nothing is bundled.
import {
  ECRClient,
  BatchGetImageCommand,
  GetDownloadUrlForLayerCommand,
} from '@aws-sdk/client-ecr';
import {
  ECSClient,
  DescribeServicesCommand,
  DescribeTaskDefinitionCommand,
} from '@aws-sdk/client-ecs';
import {
  LambdaClient,
  GetFunctionConfigurationCommand,
} from '@aws-sdk/client-lambda';

import { coreVersion, verdict } from './check.mjs';
import {
  networkProtocol,
  requiredVersion,
  stableVersions,
  xdrVerdict,
} from './xdr.mjs';

const MANIFEST_TYPES = [
  'application/vnd.docker.distribution.manifest.list.v2+json',
  'application/vnd.oci.image.index.v1+json',
  'application/vnd.docker.distribution.manifest.v2+json',
  'application/vnd.oci.image.manifest.v1+json',
];
const HUB_REPO = 'stellar/stellar-galexie';

const ecs = new ECSClient({});
const ecr = new ECRClient({});
const lambda = new LambdaClient({});

async function getJson(url, headers = {}) {
  // Without the query string: an ECR download URL carries its signature there.
  const where = url.split('?')[0];
  let response;
  try {
    response = await fetch(url, {
      headers: { accept: 'application/json', ...headers },
      signal: AbortSignal.timeout(10000),
    });
  } catch (error) {
    throw new Error(`cannot read ${where}: ${error.message}`);
  }
  if (!response.ok) {
    throw new Error(`cannot read ${where}: HTTP ${response.status}`);
  }
  return response.json();
}

/** The Galexie image of the service's current task definition. */
async function runningImage() {
  const services = await ecs.send(
    new DescribeServicesCommand({
      cluster: process.env.CLUSTER_NAME,
      services: [process.env.SERVICE_NAME],
    })
  );
  const taskDefinition = services.services?.[0]?.taskDefinition;
  if (!taskDefinition) {
    throw new Error(`cannot determine: no service ${process.env.SERVICE_NAME}`);
  }

  const described = await ecs.send(
    new DescribeTaskDefinitionCommand({ taskDefinition })
  );
  const container = described.taskDefinition.containerDefinitions.find(
    (c) => c.name === 'Galexie'
  );
  if (!container) {
    throw new Error(
      `cannot determine: no Galexie container in ${taskDefinition}`
    );
  }

  // <registry>/<repository>@sha256:…, <repository>:<tag>@sha256:… or
  // <repository>:<tag>; a digest, when present, is what runs.
  const path = container.image.slice(container.image.indexOf('/') + 1);
  const at = path.indexOf('@');
  if (at >= 0) {
    return {
      repository: path.slice(0, at).split(':')[0],
      reference: path.slice(at + 1),
    };
  }
  const [repository, tag] = path.split(':');
  return { repository, reference: tag ?? 'latest' };
}

function ecrRegistry(repository) {
  return {
    async manifest(reference) {
      const imageIds = [
        reference.startsWith('sha256:')
          ? { imageDigest: reference }
          : { imageTag: reference },
      ];
      const result = await ecr.send(
        new BatchGetImageCommand({
          repositoryName: repository,
          imageIds,
          acceptedMediaTypes: MANIFEST_TYPES,
        })
      );
      const image = result.images?.[0];
      if (!image) {
        throw new Error(
          `cannot determine: ${repository}@${reference} is not in ECR`
        );
      }
      return JSON.parse(image.imageManifest);
    },
    async blob(digest) {
      const result = await ecr.send(
        new GetDownloadUrlForLayerCommand({
          repositoryName: repository,
          layerDigest: digest,
        })
      );
      return getJson(result.downloadUrl);
    },
  };
}

async function hubRegistry() {
  const { token } = await getJson(
    `https://auth.docker.io/token?service=registry.docker.io&scope=repository:${HUB_REPO}:pull`
  );
  const base = `https://registry-1.docker.io/v2/${HUB_REPO}`;
  const auth = { authorization: `Bearer ${token}` };
  return {
    async manifest(reference) {
      return getJson(`${base}/manifests/${reference}`, {
        ...auth,
        accept: MANIFEST_TYPES.join(', '),
      });
    },
    async blob(digest) {
      return getJson(`${base}/blobs/${digest}`, auth);
    },
  };
}

/**
 * The most recently pushed tag on Docker Hub and its core, release or commit
 * build. P29's core first shipped as commit tag `c927ffc` (2026-09-24), a
 * week before the vote and the `29.0.0` tag, which came after it.
 */
async function newestOnHub() {
  const tags = await getJson(
    `https://hub.docker.com/v2/repositories/${HUB_REPO}/tags?page_size=1&ordering=last_updated`
  );
  const tag = tags.results[0].name;
  return { tag, ...(await coreVersion(await hubRegistry(), tag)) };
}

async function galexieCheck() {
  const image = await runningImage();
  const ours = await coreVersion(
    ecrRegistry(image.repository),
    image.reference
  );
  return verdict({ ours, newest: await newestOnHub() });
}

/** The stellar-xdr major the deployed indexer was built with. */
async function indexerXdrMajor() {
  const config = await lambda.send(
    new GetFunctionConfigurationCommand({
      FunctionName: process.env.INDEXER_FUNCTION_NAME,
    })
  );
  const value = config.Environment?.Variables?.STELLAR_XDR_MAJOR;
  if (!/^\d+$/.test(value ?? '')) {
    throw new Error(
      `cannot determine: ${process.env.INDEXER_FUNCTION_NAME} has STELLAR_XDR_MAJOR=${value}`
    );
  }
  return Number(value);
}

const CRATES = 'https://crates.io/api/v1/crates';
// crates.io refuses requests without a User-Agent.
const CRATES_HEADERS = {
  'user-agent':
    'soroban-block-explorer protocol watch (github.com/rumblefishdev/soroban-block-explorer)',
};

/**
 * soroban-env-host releases, newest first, with the stellar-xdr major each is
 * built on, down to the first one not newer than ours. A host reaches
 * stellar-xdr through soroban-env-common, which it pins exactly: 29.0.2 →
 * soroban-env-common =29.0.2 → stellar-xdr =28.0.0. Usually one or two
 * releases, two reads each.
 */
async function hostReleases(pinned) {
  const hosts = [];
  const listing = await getJson(`${CRATES}/soroban-env-host`, CRATES_HEADERS);
  for (const num of stableVersions(listing)) {
    const common = requiredVersion(
      await getJson(
        `${CRATES}/soroban-env-host/${num}/dependencies`,
        CRATES_HEADERS
      ),
      'soroban-env-common'
    );
    const xdr = requiredVersion(
      await getJson(
        `${CRATES}/soroban-env-common/${common}/dependencies`,
        CRATES_HEADERS
      ),
      'stellar-xdr'
    );
    hosts.push({ num, xdr: Number(xdr.split('.')[0]) });
    if (hosts.at(-1).xdr <= pinned) break;
  }
  return hosts;
}

async function xdrCheck() {
  const pinned = await indexerXdrMajor();
  const network = networkProtocol(
    await getJson('https://horizon.stellar.org/')
  );
  const lags = pinned < network.current || pinned < network.supported;
  const hosts = lags ? await hostReleases(pinned) : null;
  return xdrVerdict({ pinned, network, hosts });
}

/** A check that throws counts as one that could not check: not ok. */
async function run(check) {
  try {
    return await check();
  } catch (error) {
    return { ok: false, message: error.message };
  }
}

export async function handler() {
  // Both checks run even when one fails, so one log line says it all.
  const results = [await run(galexieCheck), await run(xdrCheck)];
  const message = results.map((r) => r.message).join('\n');

  console.log(message);
  if (results.some((r) => !r.ok)) {
    throw new Error(message);
  }
  return message;
}
