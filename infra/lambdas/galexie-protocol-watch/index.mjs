// Galexie protocol watch (task 0610): every 6 hours, compare the captive-core
// version of the Galexie that runs with the newest Galexie on Docker Hub. A
// newer core there — and any read that fails — throws, and the alarm on this
// function's Errors goes to Slack. The reason is in the log.
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
    signal: AbortSignal.timeout(10000),
  });
  if (!response.ok) {
    throw new Error(`cannot read ${url}: HTTP ${response.status}`);
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

  // <account>.dkr.ecr.<region>.amazonaws.com/<repository>@sha256:<digest>, or :<tag>
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

export async function handler() {
  const image = await runningImage();
  const ours = await coreVersion(
    ecrRegistry(image.repository),
    image.reference
  );
  const newest = await newestOnHub();
  const result = verdict({ ours, newest });

  console.log(result.message);
  if (!result.ok) {
    throw new Error(result.message);
  }
  return result.message;
}
