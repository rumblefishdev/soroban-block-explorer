// Galexie protocol watch (task 0610): every 30 minutes, compare the captive-core
// version of the Galexie that runs with the protocol the network's core
// supports. Any answer other than OK — and any read that fails — throws, and
// the alarm on this function's Errors goes to Slack. The reason is in the log.
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

import { coreVersion, verdict } from './check.mjs';

const MANIFEST_TYPES = [
  'application/vnd.docker.distribution.manifest.list.v2+json',
  'application/vnd.oci.image.index.v1+json',
  'application/vnd.docker.distribution.manifest.v2+json',
  'application/vnd.oci.image.manifest.v1+json',
];
const HUB_REPO = 'stellar/stellar-galexie';

const ecs = new ECSClient({});
const ecr = new ECRClient({});

async function getJson(url, headers = {}) {
  const response = await fetch(url, {
    headers: { accept: 'application/json', ...headers },
    signal: AbortSignal.timeout(15000),
  });
  if (!response.ok) {
    throw new Error(`cannot read ${url}: HTTP ${response.status}`);
  }
  return response.json();
}

async function networkProtocols() {
  const root = await getJson(process.env.HORIZON_URL);
  const network = {
    current: root.current_protocol_version,
    supported: root.core_supported_protocol_version,
  };
  if (
    !Number.isInteger(network.current) ||
    !Number.isInteger(network.supported)
  ) {
    throw new Error(
      `cannot determine: unexpected Horizon root ${JSON.stringify(network)}`
    );
  }
  return network;
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

  // <account>.dkr.ecr.<region>.amazonaws.com/<repository>@sha256:<digest>, or :<tag>
  const match = container.image.match(/^[^/]+\/([^@:]+)[@:](.+)$/);
  if (!match) {
    throw new Error(`cannot determine: unexpected image ${container.image}`);
  }
  return { repository: match[1], reference: match[2] };
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
    manifest: (reference) =>
      getJson(`${base}/manifests/${reference}`, {
        ...auth,
        accept: MANIFEST_TYPES.join(', '),
      }),
    blob: (digest) => getJson(`${base}/blobs/${digest}`, auth),
  };
}

/**
 * The most recently pushed tag on Docker Hub, release or commit build. P29's
 * core first shipped as commit tag `c927ffc` (2026-09-24), a week before the
 * `29.0.0` tag, so release tags alone would hide the earliest image.
 */
async function newestOnHub() {
  try {
    const tags = await getJson(
      `https://hub.docker.com/v2/repositories/${HUB_REPO}/tags?page_size=1&ordering=last_updated`
    );
    const tag = tags.results[0].name;
    return { tag, ...(await coreVersion(await hubRegistry(), tag)) };
  } catch (error) {
    return { error: error.message };
  }
}

export async function handler() {
  const network = await networkProtocols();
  const image = await runningImage();
  const ours = await coreVersion(
    ecrRegistry(image.repository),
    image.reference
  );
  const result = verdict({ network, ours, newest: await newestOnHub() });

  console.log(result.message);
  if (!result.ok) {
    throw new Error(result.message);
  }
  return result.message;
}
