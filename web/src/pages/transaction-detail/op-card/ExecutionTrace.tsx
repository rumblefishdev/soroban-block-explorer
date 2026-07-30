import CodeIcon from '@mui/icons-material/Code';
import ExpandMoreIcon from '@mui/icons-material/ExpandMore';
import { Box, Collapse, IconButton, Stack, Typography } from '@mui/material';
import { Chip, IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';
import { useState } from 'react';

import type { XdrEventDto } from '@rumblefish/api-types';

import { HighlightedJson } from './HighlightedJson.js';

/** One executed call, reconstructed from the host-VM diagnostic trace. */
export interface TraceNode {
  fnName: string;
  /** Called contract as a C-strkey (decoded from the fn_call bytes topic). */
  contractId: string | null;
  /** fn_call event data — the call arguments (typed JSON). */
  args: unknown;
  /** fn_return event data — undefined while the call never returned. */
  returnValue: unknown;
  /** True when the trace ended before this call returned — on a failed
   *  transaction this marks where execution actually stopped. */
  unfinished: boolean;
  children: TraceNode[];
  /** Contract events raised while this call was on top of the stack. */
  events: XdrEventDto[];
}

interface TypedVal {
  type?: unknown;
  value?: unknown;
}

function typedTopic(event: XdrEventDto, index: number): TypedVal | null {
  const topic: unknown = event.topics[index];
  return topic != null && typeof topic === 'object'
    ? (topic as TypedVal)
    : null;
}

function symTopic(event: XdrEventDto, index: number): string | null {
  const topic = typedTopic(event, index);
  return topic?.type === 'sym' && typeof topic.value === 'string'
    ? topic.value
    : null;
}

const BASE32_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

function crc16xmodem(bytes: Uint8Array): number {
  let crc = 0;
  for (const byte of bytes) {
    crc ^= byte << 8;
    for (let bit = 0; bit < 8; bit++) {
      crc = crc & 0x8000 ? ((crc << 1) ^ 0x1021) & 0xffff : (crc << 1) & 0xffff;
    }
  }
  return crc;
}

function base32(bytes: Uint8Array): string {
  let out = '';
  let buffer = 0;
  let bits = 0;
  for (const byte of bytes) {
    buffer = (buffer << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      out += BASE32_ALPHABET[(buffer >> (bits - 5)) & 31];
      bits -= 5;
    }
  }
  if (bits > 0) out += BASE32_ALPHABET[(buffer << (5 - bits)) & 31];
  return out;
}

/** SEP-23 strkey for a contract address: version byte 0x10 ('C') + 32-byte
 *  hash + CRC16-XModem (little-endian), base32 without padding. The fn_call
 *  topic carries the called contract as raw bytes; everywhere else in the
 *  API contracts already arrive as strkeys, so this is the one place the
 *  frontend needs to encode. */
export function contractStrkeyFromBase64(b64: string): string | null {
  let bin: string;
  try {
    bin = atob(b64);
  } catch {
    return null;
  }
  if (bin.length !== 32) return null;
  const payload = new Uint8Array(35);
  payload[0] = 0x10;
  for (let i = 0; i < 32; i++) payload[i + 1] = bin.charCodeAt(i);
  const crc = crc16xmodem(payload.subarray(0, 33));
  payload[33] = crc & 0xff;
  payload[34] = crc >> 8;
  return base32(payload);
}

function calledContract(event: XdrEventDto): string | null {
  const topic = typedTopic(event, 1);
  if (topic?.type === 'bytes' && typeof topic.value === 'string') {
    return contractStrkeyFromBase64(topic.value);
  }
  // Defensive: some renderers emit the address form directly.
  if (topic?.type === 'address' && typeof topic.value === 'string') {
    return topic.value;
  }
  return null;
}

/** First topic names well-formed token events (transfer, mint, burn, …). */
export function traceEventLabel(event: XdrEventDto): string {
  return symTopic(event, 0) ?? event.event_type;
}

/** Reconstruct the execution call tree from the diagnostic event stream.
 *
 *  The host VM emits `fn_call` when a function is entered and `fn_return`
 *  when it exits, in stream order, with the contract events raised in
 *  between — so a plain stack walk rebuilds the exact execution: push on
 *  call, pop on return, attach everything else to the call currently on
 *  top. `core_metrics` are host resource counters, not calls or effects.
 *
 *  A failed transaction's trace stops mid-flight: calls left on the stack
 *  at the end never returned and are marked `unfinished` — that is the
 *  truthful "execution stopped here", unlike the auth tree (see CallTree).
 */
export function buildExecutionTrace(
  events: readonly XdrEventDto[]
): TraceNode[] {
  const roots: TraceNode[] = [];
  const stack: TraceNode[] = [];
  for (const event of events) {
    const kind = symTopic(event, 0);
    if (kind === 'fn_call') {
      const node: TraceNode = {
        fnName: symTopic(event, 2) ?? 'call',
        contractId: calledContract(event),
        args: event.data,
        returnValue: undefined,
        unfinished: false,
        children: [],
        events: [],
      };
      (stack.length > 0 ? stack[stack.length - 1].children : roots).push(node);
      stack.push(node);
    } else if (kind === 'fn_return') {
      const node = stack.pop();
      if (node != null) node.returnValue = event.data;
    } else if (kind === 'core_metrics') {
      // Host resource counters — not part of the call story.
    } else if (stack.length > 0) {
      stack[stack.length - 1].events.push(event);
    }
    // Events outside any call (fee events etc.) stay in the Events section.
  }
  for (const node of stack) node.unfinished = true;
  return roots;
}

/** Total call count, for the section header and collapsed-branch badges. */
export function traceCallCount(nodes: readonly TraceNode[]): number {
  return nodes.reduce(
    (sum, node) => sum + 1 + traceCallCount(node.children),
    0
  );
}

/** Same-label neighbours collapse into one `label ×N` chip — a failing call
 *  carries the contract error AND the host's escalation copy, both topic
 *  `error`; two identical chips read as a rendering bug. Full per-event data
 *  stays in the Events table. */
function groupEventLabels(
  events: readonly XdrEventDto[]
): { label: string; count: number }[] {
  const groups: { label: string; count: number }[] = [];
  for (const event of events) {
    const label = traceEventLabel(event);
    const last = groups[groups.length - 1];
    if (last != null && last.label === label) last.count += 1;
    else groups.push({ label, count: 1 });
  }
  return groups;
}

/** One argument as short inline text, or null when it wouldn't fit a row:
 *  addresses shorten to GC4Q…K7XQ form, numbers/symbols pass through. */
function shortVal(value: unknown): string | null {
  if (value == null || typeof value !== 'object') return null;
  const { type, value: inner } = value as TypedVal;
  if (type === 'address' && typeof inner === 'string' && inner.length > 12) {
    return `${inner.slice(0, 4)}…${inner.slice(-4)}`;
  }
  if (typeof inner === 'number' || typeof inner === 'boolean') {
    return String(inner);
  }
  if (typeof inner === 'string') {
    // Big ints (i128/u128…) arrive as decimal strings; symbols stay short.
    if (/^-?\d+$/.test(inner)) return inner;
    if (type === 'sym' && inner.length <= 12) return inner;
  }
  return null;
}

/** Hybrid header (option C): literal args inline when every one is
 *  short and the whole list fits a row — `transfer(GC4Q…K7XQ, 13171)`;
 *  otherwise fall back to the count — `swap_collateral(6)`. The full typed
 *  args stay behind the per-node disclosure either way. */
function argsSummary(args: unknown): string {
  if (args == null) return '0';
  const typed = args as TypedVal;
  if (typed.type !== 'vec' || !Array.isArray(typed.value)) return '1';
  const parts: string[] = [];
  for (const element of typed.value) {
    const short = shortVal(element);
    if (short == null) return String(typed.value.length);
    parts.push(short);
  }
  const joined = parts.join(', ');
  return joined.length <= 40 ? joined : String(typed.value.length);
}

/** Short inline `→ value` for scalar returns; structured values stay behind
 *  the per-node disclosure. */
function scalarReturn(value: unknown): string | null {
  if (value == null) return null;
  let inner: unknown = value;
  const typed = value as { type?: unknown; value?: unknown };
  if (typeof typed.type === 'string') {
    if (typed.type === 'void') return null;
    if (typed.type === 'vec' || typed.type === 'map') return null;
    inner = typed.value;
  }
  if (
    typeof inner === 'string' ||
    typeof inner === 'number' ||
    typeof inner === 'boolean'
  ) {
    const text = String(inner);
    return text.length <= 24 ? text : null;
  }
  return null;
}

function TraceNodeRow({ node, depth }: { node: TraceNode; depth: number }) {
  // Deep branches start folded so a 40-call DeFi trace reads as a summary
  // first; the top two levels are the story. Unfinished branches ALWAYS
  // start open — the path to the trap point must be visible without
  // digging, or the single "stopped here" marker would hide in a fold.
  const [childrenOpen, setChildrenOpen] = useState(
    depth < 2 || node.unfinished
  );
  const [detailsOpen, setDetailsOpen] = useState(false);
  const hasChildren = node.children.length > 0;
  const inlineReturn = scalarReturn(node.returnValue);
  const hasDetails = node.args != null || node.returnValue != null;

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
        {hasChildren ? (
          <IconButton
            size="small"
            aria-label={childrenOpen ? 'Collapse calls' : 'Expand calls'}
            aria-expanded={childrenOpen}
            onClick={() => setChildrenOpen((open) => !open)}
            sx={{ p: 0.25 }}
          >
            <ExpandMoreIcon
              sx={(theme) => ({
                fontSize: 16,
                color: theme.palette.text.tertiary,
                transform: childrenOpen ? 'none' : 'rotate(-90deg)',
                transition: 'transform 120ms ease',
              })}
            />
          </IconButton>
        ) : (
          <Box sx={{ width: 20 }} />
        )}
        <Typography
          variant="bodyMonoSmMedium"
          sx={(theme) => ({
            color: theme.palette.text.primary,
            // Rows scroll inside the strip's overflowX container (RouteStrip
            // pattern) — wrapping mid-token on narrow viewports is unreadable.
            whiteSpace: 'nowrap',
          })}
        >
          {node.fnName}({argsSummary(node.args)})
        </Typography>
        {node.contractId != null && (
          <Typography variant="bodyXsRegular" component="span">
            <IdentifierDisplay value={node.contractId} type="contract" />
          </Typography>
        )}
        {inlineReturn != null && (
          <Typography
            variant="bodyMonoSmRegular"
            sx={(theme) => ({
              color: theme.palette.text.secondary,
              whiteSpace: 'nowrap',
            })}
          >
            → {inlineReturn}
          </Typography>
        )}
        {groupEventLabels(node.events).map((group, index) => (
          <Chip
            key={`${group.label}-${index}`}
            size="sm"
            color="blue"
            label={
              group.count > 1 ? `${group.label} ×${group.count}` : group.label
            }
          />
        ))}
        {hasChildren && !childrenOpen && (
          <Chip
            size="sm"
            color="neutral"
            label={`${traceCallCount(node.children)} calls`}
          />
        )}
        {/* The whole unfinished stack path is marked in the model, but the
            chip renders only at the DEEPEST unfinished call — repeating it
            on every ancestor reads as noise (review finding); the nesting
            already shows the path. */}
        {node.unfinished && !node.children.some((c) => c.unfinished) && (
          <Chip size="sm" color="error" label="stopped here" />
        )}
        {hasDetails && (
          <IconButton
            size="small"
            aria-label="Call arguments and return value"
            aria-expanded={detailsOpen}
            onClick={() => setDetailsOpen((open) => !open)}
            sx={{ p: 0.25 }}
          >
            <CodeIcon
              sx={(theme) => ({
                fontSize: 14,
                color: detailsOpen
                  ? theme.palette.text.primary
                  : theme.palette.text.tertiary,
              })}
            />
          </IconButton>
        )}
      </Stack>
      {hasDetails && (
        <Collapse in={detailsOpen} unmountOnExit>
          <Box
            sx={(theme) => ({
              ml: depth * 2.5 + 3,
              my: 0.5,
              px: 1.5,
              py: 1,
              borderRadius: `${theme.shape.radius.s}px`,
              backgroundColor: theme.palette.surface.background,
            })}
          >
            {node.args != null && (
              <>
                <Typography
                  variant="bodyXsRegular"
                  sx={(theme) => ({ color: theme.palette.text.tertiary })}
                >
                  arguments
                </Typography>
                <HighlightedJson value={node.args} />
              </>
            )}
            {node.returnValue != null && (
              <>
                <Typography
                  variant="bodyXsRegular"
                  sx={(theme) => ({
                    color: theme.palette.text.tertiary,
                    mt: node.args != null ? 1 : 0,
                  })}
                >
                  return
                </Typography>
                <HighlightedJson value={node.returnValue} />
              </>
            )}
          </Box>
        </Collapse>
      )}
      <Collapse in={childrenOpen} unmountOnExit>
        {node.children.map((child, index) => (
          <TraceNodeRow key={index} node={child} depth={depth + 1} />
        ))}
      </Collapse>
    </>
  );
}

/** The EXECUTED call tree, rebuilt from the host-VM diagnostic trace —
 *  a superset of the auth tree and the only source that can truthfully say
 *  where a failed execution stopped (see `buildExecutionTrace`). */
export function ExecutionTrace({ nodes }: { nodes: TraceNode[] }) {
  return (
    <Box sx={{ overflowX: 'auto' }}>
      {nodes.map((node, index) => (
        <TraceNodeRow key={index} node={node} depth={0} />
      ))}
    </Box>
  );
}
