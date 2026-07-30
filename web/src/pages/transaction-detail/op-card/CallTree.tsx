import { Box, Stack, Typography } from '@mui/material';
import { IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';

export interface CallNode {
  contractId: string | null;
  functionName: string | null;
  argCount: number;
  /** Parsed but NOT rendered per node: the backend builds this tree from the
   *  transaction's AUTH entries and stamps every node with the whole
   *  transaction's verdict (`invocation.rs` — "derived from the parent
   *  transaction's success status"), so a per-node ✓/✗ here would be the
   *  0444 lie reborn. Real per-node verdicts need the diagnostic execution
   *  tree on the backend first. */
  successful: boolean | null;
  children: CallNode[];
}

function asNode(value: unknown): CallNode | null {
  if (value == null || typeof value !== 'object') return null;
  const raw = value as Record<string, unknown>;
  return {
    contractId: typeof raw.contractId === 'string' ? raw.contractId : null,
    functionName:
      typeof raw.functionName === 'string' ? raw.functionName : null,
    argCount: Array.isArray(raw.args) ? raw.args.length : 0,
    successful: typeof raw.successful === 'boolean' ? raw.successful : null,
    children: parseOperationTree(raw.children),
  };
}

/** `heavy.operation_tree` is untyped JSON: an array of per-auth-entry
 *  invocation trees. Parse defensively; anything malformed just drops out. */
export function parseOperationTree(value: unknown): CallNode[] {
  if (!Array.isArray(value)) return [];
  return value.map(asNode).filter((node): node is CallNode => node != null);
}

function CallNodeRow({ node, depth }: { node: CallNode; depth: number }) {
  return (
    <>
      <Stack
        direction="row"
        spacing={0.75}
        alignItems="center"
        sx={(theme) => ({
          pl: depth * 2.5,
          py: 0.25,
          borderLeft:
            depth > 0 ? `1px dashed ${theme.palette.stroke.default}` : 'none',
          ml: depth > 0 ? 1 : 0,
        })}
      >
        <Typography
          variant="bodyMonoSmMedium"
          sx={(theme) => ({ color: theme.palette.text.primary })}
        >
          {node.functionName ?? 'call'}({node.argCount})
        </Typography>
        {node.contractId != null && (
          <Typography variant="bodyXsRegular" component="span">
            <IdentifierDisplay value={node.contractId} type="contract" />
          </Typography>
        )}
      </Stack>
      {node.children.map((child, index) => (
        <CallNodeRow key={index} node={child} depth={depth + 1} />
      ))}
    </>
  );
}

/** The AUTHORIZED call tree — what the transaction was signed to do, not an
 *  execution trace. The section label must say so (see `CallNode.successful`). */
export function CallTree({ nodes }: { nodes: CallNode[] }) {
  return (
    <Box sx={{ overflowX: 'auto' }}>
      {nodes.map((node, index) => (
        <CallNodeRow key={index} node={node} depth={0} />
      ))}
    </Box>
  );
}
