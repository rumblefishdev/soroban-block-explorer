import type {
  E3ResponseTransactionDetailLight,
  OperationItem,
  XdrOperationDto,
} from '@rumblefish/api-types';
import { Stack, Typography } from '@mui/material';
import type { FlowNode } from '@rumblefish/soroban-block-explorer-ui';

import { humanizeOp } from './humanizeOp.js';

interface BuildNodesInput {
  tx: E3ResponseTransactionDetailLight;
  light: OperationItem;
  heavy: XdrOperationDto | null;
}

function asObject(value: unknown): Record<string, unknown> | null {
  return value != null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function asString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
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
  // one. The verdict is the TRANSACTION's, because that is the only one the API
  // exposes (per-operation result codes are task 0352's Step 6). That is not a
  // fudge — Stellar applies a transaction atomically, so when it fails no
  // operation took effect, which is exactly what a reader needs to know. It
  // does mean we cannot say WHICH operation was at fault.
  const ok = tx.successful;
  const resultNode: FlowNode = {
    id: 'result',
    kind: ok ? 'result' : 'result-failed',
    title: ok ? 'Result · Success' : 'Result · Failed',
    summary: ok ? (
      humanizeOp(light, heavy)
    ) : (
      <Stack spacing={0.25}>
        <Typography variant="bodySmRegular" sx={{ color: 'inherit' }}>
          Transaction failed — this operation was not applied.
        </Typography>
        <Typography
          variant="bodySmRegular"
          sx={{ color: 'inherit', opacity: 0.85 }}
        >
          {humanizeOp(light, heavy)}
        </Typography>
      </Stack>
    ),
  };

  if (light.type_name === 'INVOKE_HOST_FUNCTION') {
    // The parser emits camelCase keys (`functionName`), not snake_case.
    const rootFn = asString(asObject(heavy?.details)?.functionName);

    if (light.contract_id != null) {
      tree.push({
        id: 'contract-root',
        kind: 'contract',
        title: 'Contract',
        identifier: { value: light.contract_id, type: 'contract' },
        detail: rootFn != null ? `· ${rootFn}()` : undefined,
        connectorLabel: 'Invoke',
      });
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
