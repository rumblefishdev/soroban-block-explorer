import ArrowDownwardIcon from '@mui/icons-material/ArrowDownward';
import KeyboardArrowDownIcon from '@mui/icons-material/KeyboardArrowDown';
import { Box, Collapse, Stack, Typography } from '@mui/material';
import type { Theme } from '@mui/material/styles';
import type { ReactNode } from 'react';
import { useState } from 'react';

import { IdentifierDisplay } from '../identifiers/IdentifierDisplay.js';
import type { EntityType } from '../identifiers/types.js';
import { scales } from '../theme/colors.js';
import { monoFontFamily } from '../theme/typography.js';

/**
 * Node roles in a transaction's operation flow. Soroban invocations reuse the
 * `contract` kind and nest via `children` — there is no separate call-tree
 * component (the Figma design renders one unified tree).
 */
export type FlowNodeKind =
  | 'account'
  | 'operation'
  | 'contract'
  | 'destination'
  | 'result';

export interface FlowNodeIdentifier {
  value: string;
  type: EntityType;
}

export interface FlowNode {
  id: string;
  kind: FlowNodeKind;
  /** Heading line, e.g. "Source account", "Contract", "Result". */
  title: ReactNode;
  /** Linked, truncated identifier (account/contract id). */
  identifier?: FlowNodeIdentifier;
  /** Inline text after the identifier, e.g. a function name `swap()`. */
  detail?: ReactNode;
  /** Human-readable summary, e.g. "Swapped 100 XLM for 1,250 USDC". */
  summary?: ReactNode;
  /** Connector label shown above this node, e.g. "Invoke" / "Calls". */
  connectorLabel?: ReactNode;
  /** Nested invocations / affected entities. */
  children?: readonly FlowNode[];
  /** Whether children start expanded. Defaults to true. */
  defaultExpanded?: boolean;
}

export interface OperationFlowTreeProps {
  /** Top-level node sequence (typically rooted at the source account). */
  nodes: readonly FlowNode[];
}

interface NodePalette {
  backgroundColor: string;
  borderColor: string;
  color: string;
}

function nodeStyle(theme: Theme, kind: FlowNodeKind): NodePalette {
  switch (kind) {
    case 'contract':
      return {
        backgroundColor: scales.blue[900],
        borderColor: scales.blue[600],
        color: theme.palette.common.white,
      };
    case 'destination':
      return {
        backgroundColor: scales.violet[900],
        borderColor: scales.violet[600],
        color: theme.palette.common.white,
      };
    case 'result':
      return {
        backgroundColor: scales.green[950],
        borderColor: scales.green[600],
        color: theme.palette.common.white,
      };
    case 'account':
    case 'operation':
    default:
      return {
        backgroundColor: theme.palette.surface.background,
        borderColor: theme.palette.stroke.defaultHover,
        color: theme.palette.text.primary,
      };
  }
}

function FlowConnector({ label }: { label: ReactNode }) {
  return (
    <Stack
      direction="row"
      spacing={0.5}
      alignItems="center"
      sx={{ py: 1, pl: 0.5 }}
    >
      <ArrowDownwardIcon
        sx={(theme) => ({ fontSize: 14, color: theme.palette.text.tertiary })}
      />
      <Typography
        variant="bodyXsMedium"
        sx={(theme) => ({ color: theme.palette.text.tertiary })}
      >
        {label}
      </Typography>
    </Stack>
  );
}

function FlowNodeCard({ node }: { node: FlowNode }) {
  const children = node.children ?? [];
  const hasChildren = children.length > 0;
  const [expanded, setExpanded] = useState(node.defaultExpanded ?? true);

  return (
    <Box>
      <Box
        sx={(theme) => {
          const palette = nodeStyle(theme, node.kind);
          return {
            backgroundColor: palette.backgroundColor,
            color: palette.color,
            border: `1px solid ${palette.borderColor}`,
            borderRadius: `${theme.shape.radius.md}px`,
            px: 2,
            py: 1.5,
          };
        }}
      >
        <Stack
          direction="row"
          alignItems="flex-start"
          justifyContent="space-between"
          spacing={1}
        >
          <Stack spacing={0.5} sx={{ minWidth: 0 }}>
            <Typography variant="heading6SemiBold" sx={{ color: 'inherit' }}>
              {node.title}
            </Typography>
            {(node.identifier || node.detail !== undefined) && (
              <Box
                sx={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 0.75,
                  flexWrap: 'wrap',
                }}
              >
                {node.identifier && (
                  <IdentifierDisplay
                    value={node.identifier.value}
                    type={node.identifier.type}
                    tone="inherit"
                  />
                )}
                {node.detail !== undefined && (
                  <Typography
                    component="span"
                    sx={{
                      fontFamily: monoFontFamily,
                      fontSize: 14,
                      color: 'inherit',
                    }}
                  >
                    {node.detail}
                  </Typography>
                )}
              </Box>
            )}
            {node.summary !== undefined && (
              <Typography
                variant="bodySmRegular"
                sx={{ color: 'inherit', opacity: 0.85 }}
              >
                {node.summary}
              </Typography>
            )}
          </Stack>
          {hasChildren && (
            <Box
              component="button"
              type="button"
              aria-expanded={expanded}
              aria-label={
                expanded ? 'Collapse nested calls' : 'Expand nested calls'
              }
              onClick={() => setExpanded((value) => !value)}
              sx={(theme) => ({
                display: 'inline-flex',
                p: 0.25,
                m: 0,
                border: 'none',
                background: 'transparent',
                color: 'inherit',
                cursor: 'pointer',
                borderRadius: `${theme.shape.radius.xs}px`,
                flexShrink: 0,
                '&:focus-visible': {
                  outline: '2px solid currentColor',
                  outlineOffset: 2,
                },
              })}
            >
              <KeyboardArrowDownIcon
                sx={{
                  fontSize: 20,
                  transition: 'transform 150ms ease',
                  transform: expanded ? 'rotate(0deg)' : 'rotate(-90deg)',
                }}
              />
            </Box>
          )}
        </Stack>
      </Box>
      {hasChildren && (
        <Collapse in={expanded} unmountOnExit>
          <Box
            sx={(theme) => ({
              ml: 2,
              pl: 2,
              pb: 2,
              borderLeft: `1px dashed ${theme.palette.stroke.default}`,
              display: 'flex',
              flexDirection: 'column',
              gap: 1.5,
            })}
          >
            {children.map((child) => (
              <FlowNodeBlock key={child.id} node={child} />
            ))}
          </Box>
        </Collapse>
      )}
    </Box>
  );
}

function FlowNodeBlock({ node }: { node: FlowNode }) {
  return (
    <Box>
      {node.connectorLabel !== undefined && (
        <FlowConnector label={node.connectorLabel} />
      )}
      <FlowNodeCard node={node} />
    </Box>
  );
}

/**
 * Unified transaction operation flow tree. Renders source account, classic
 * operations, contracts and results as typed, colour-coded node cards joined
 * by labelled connectors. Soroban contract-to-contract calls nest as indented
 * children. Presentational — the consuming page maps the API response
 * (`operation_tree` / `invocations`) into `FlowNode`s.
 */
export function OperationFlowTree({ nodes }: OperationFlowTreeProps) {
  return (
    <Box>
      {nodes.map((node) => (
        <FlowNodeBlock key={node.id} node={node} />
      ))}
    </Box>
  );
}
