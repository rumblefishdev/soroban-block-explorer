import type {
  E3ResponseTransactionDetailLight,
  OperationItem,
  XdrOperationDto,
} from '@rumblefish/api-types';
import { Stack, Typography } from '@mui/material';
import type { FlowNode } from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { humanizeOp } from './humanizeOp.js';

interface BuildNodesInput {
  tx: E3ResponseTransactionDetailLight;
  light: OperationItem;
  heavy: XdrOperationDto | null;
}

interface NestedCallShape {
  contract_id?: unknown;
  contract_label?: unknown;
  function_name?: unknown;
  destination_account?: unknown;
  destination_summary?: unknown;
  invocations?: unknown;
}

function asObject(value: unknown): Record<string, unknown> | null {
  return value != null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function asString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function asNestedCalls(value: unknown): NestedCallShape[] {
  return Array.isArray(value) ? (value as NestedCallShape[]) : [];
}

function contractTitle(label: string | null): ReactNode {
  if (label == null) return 'Contract';
  return (
    <Stack spacing={0.25} sx={{ minWidth: 0 }}>
      <Typography variant="heading6SemiBold" sx={{ color: 'inherit' }}>
        Contract
      </Typography>
      <Typography
        variant="bodySmRegular"
        sx={{ color: 'inherit', opacity: 0.85 }}
      >
        {label}
      </Typography>
    </Stack>
  );
}

function buildNestedChildren(
  calls: NestedCallShape[],
  pathPrefix: string,
  destinationCollector: { value: string | null; summary: string | null }
): FlowNode[] {
  const nodes: FlowNode[] = [];
  calls.forEach((call, index) => {
    const contractId = asString(call.contract_id);
    const fnName = asString(call.function_name);
    const label = asString(call.contract_label);
    const dest = asString(call.destination_account);
    const destSummary = asString(call.destination_summary);
    if (dest != null) {
      destinationCollector.value = dest;
      destinationCollector.summary = destSummary;
    }
    if (contractId != null) {
      nodes.push({
        id: `${pathPrefix}.call-${index}`,
        kind: 'contract',
        title: contractTitle(label),
        identifier: { value: contractId, type: 'contract' },
        detail: fnName != null ? `· ${fnName}()` : undefined,
        connectorLabel: 'Calls',
      });
    }
    const nestedRaw = asNestedCalls(call.invocations);
    if (nestedRaw.length > 0) {
      nodes.push(
        ...buildNestedChildren(
          nestedRaw,
          `${pathPrefix}.call-${index}`,
          destinationCollector
        )
      );
    }
  });
  return nodes;
}

function buildResultSummary(details: unknown, fallback: string): ReactNode {
  const obj = asObject(details);
  const line1 = asString(obj?.summary_line_1);
  const line2 = asString(obj?.summary_line_2);
  if (line1 == null && line2 == null) return fallback;
  return (
    <Stack spacing={0.25}>
      {line1 != null && (
        <Typography
          variant="bodySmRegular"
          sx={{ color: 'inherit', opacity: 0.9 }}
        >
          {line1}
        </Typography>
      )}
      {line2 != null && (
        <Typography
          variant="bodyMonoSmRegular"
          sx={{ color: 'inherit', opacity: 0.85 }}
        >
          {line2}
        </Typography>
      )}
    </Stack>
  );
}

export function toFlowNodes({ tx, light, heavy }: BuildNodesInput): FlowNode[] {
  const tree: FlowNode[] = [];

  const source = light.source_account ?? tx.source_account;
  if (source != null) {
    tree.push({
      id: 'source',
      kind: 'account',
      title: 'Source account',
      identifier: { value: source, type: 'account' },
      summary: 'Initiates transaction',
    });
  }

  // The node is titled "Result" and painted like a verdict, so it has to carry
  // one. Until task 0444 it was hardcoded green and its text described what the
  // operation *did*, never whether it worked — two separate reporters read a
  // green "Result" on a failed transaction as a success.
  //
  // The verdict is the TRANSACTION's, because that is the only one we have:
  // per-operation result codes are not exposed by the API (see task 0352's
  // Step 6). That is not a fudge — Stellar applies a transaction atomically, so
  // when it fails no operation took effect, which is exactly what a reader
  // needs to know. It does mean we cannot say WHICH operation was at fault.
  const ok = tx.successful;
  const resultNode: FlowNode = {
    id: 'result',
    kind: ok ? 'result' : 'result-failed',
    title: ok ? 'Result · Success' : 'Result · Failed',
    summary: ok ? (
      buildResultSummary(heavy?.details, humanizeOp(light, heavy))
    ) : (
      <Stack spacing={0.25}>
        <Typography variant="bodySmRegular" sx={{ color: 'inherit' }}>
          Transaction failed — this operation was not applied.
        </Typography>
        <Typography
          variant="bodySmRegular"
          sx={{ color: 'inherit', opacity: 0.85 }}
        >
          {buildResultSummary(heavy?.details, humanizeOp(light, heavy))}
        </Typography>
      </Stack>
    ),
  };

  const destinationCollector: {
    value: string | null;
    summary: string | null;
  } = { value: null, summary: null };

  if (light.type_name === 'INVOKE_HOST_FUNCTION') {
    const detailsObj = asObject(heavy?.details);
    const rootFn = asString(detailsObj?.function_name);
    const rootLabel = asString(detailsObj?.contract_label);

    const nestedCalls = buildNestedChildren(
      asNestedCalls(detailsObj?.invocations),
      'contract-root',
      destinationCollector
    );

    const children: FlowNode[] = [...nestedCalls];
    if (destinationCollector.value != null) {
      children.push({
        id: 'destination',
        kind: 'destination',
        title: 'Destination account',
        identifier: { value: destinationCollector.value, type: 'account' },
        summary: destinationCollector.summary ?? undefined,
      });
    }

    if (light.contract_id != null) {
      tree.push({
        id: 'contract-root',
        kind: 'contract',
        title: contractTitle(rootLabel),
        identifier: { value: light.contract_id, type: 'contract' },
        detail: rootFn != null ? `· ${rootFn}()` : undefined,
        connectorLabel: 'Invoke',
        children,
      });
    } else {
      tree.push(...children);
    }
    tree.push(resultNode);
    return tree;
  }

  if (light.destination_account != null) {
    const destinationChild: FlowNode = {
      id: 'destination',
      kind: 'destination',
      title: 'Destination account',
      identifier: { value: light.destination_account, type: 'account' },
      connectorLabel: 'Sends to',
    };
    if (tree.length > 0) {
      tree[0] = { ...tree[0], children: [destinationChild] };
    } else {
      tree.push(destinationChild);
    }
  }
  tree.push(resultNode);
  return tree;
}
