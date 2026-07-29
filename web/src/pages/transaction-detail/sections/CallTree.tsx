import { Box, Stack, Typography } from '@mui/material';
import { IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';

export interface CallNode {
  contractId: string | null;
  functionName: string | null;
  argCount: number;
  /** Per-node verdict from the parser (`operation_tree` carries it) — lets
   *  the tree pinpoint WHICH nested call failed (spec D11, Tenderly-lite). */
  successful: boolean | null;
  children: CallNode[];
}

function asNode(value: unknown): CallNode | null {
  if (value == null || typeof value !== 'object') return null;
  const raw = value as Record<string, unknown>;
  const children = Array.isArray(raw.children)
    ? raw.children
        .map(asNode)
        .filter((child): child is CallNode => child != null)
    : [];
  return {
    contractId: typeof raw.contractId === 'string' ? raw.contractId : null,
    functionName:
      typeof raw.functionName === 'string' ? raw.functionName : null,
    argCount: Array.isArray(raw.args) ? raw.args.length : 0,
    successful: typeof raw.successful === 'boolean' ? raw.successful : null,
    children,
  };
}

/** `heavy.operation_tree` is untyped JSON: an array of per-auth-entry
 *  invocation trees. Parse defensively; anything malformed just drops out. */
export function parseOperationTree(value: unknown): CallNode[] {
  if (!Array.isArray(value)) return [];
  return value.map(asNode).filter((node): node is CallNode => node != null);
}

function CallNodeRow({ node, depth }: { node: CallNode; depth: number }) {
  const failed = node.successful === false;
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
          sx={(theme) => ({
            color: failed
              ? theme.palette.text.error
              : theme.palette.text.primary,
          })}
        >
          {node.functionName ?? 'call'}({node.argCount})
        </Typography>
        {node.contractId != null && (
          <Typography variant="bodyXsRegular" component="span">
            <IdentifierDisplay value={node.contractId} type="contract" />
          </Typography>
        )}
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({
            color: failed
              ? theme.palette.text.error
              : theme.palette.text.success,
          })}
        >
          {node.successful == null ? '' : failed ? '✗ failed here' : '✓'}
        </Typography>
      </Stack>
      {node.children.map((child, index) => (
        <CallNodeRow key={index} node={child} depth={depth + 1} />
      ))}
    </>
  );
}

export function CallTree({ nodes }: { nodes: CallNode[] }) {
  return (
    <Box sx={{ overflowX: 'auto' }}>
      {nodes.map((node, index) => (
        <CallNodeRow key={index} node={node} depth={0} />
      ))}
    </Box>
  );
}
