import CodeIcon from '@mui/icons-material/Code';
import ExpandMoreIcon from '@mui/icons-material/ExpandMore';
import { Box, Collapse, IconButton, Stack, Typography } from '@mui/material';
import { Chip, IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
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
  /** Everything that happened inside this call, in exact stream order:
   *  sub-calls AND the events the call announced, interleaved — the
   *  chronology is data (a transfer may fire between two sub-calls), so the
   *  model must not split them into two lists (the unified-row design;
   *  same node model as Phalcon's invocation flow). */
  children: TraceChild[];
}

export type TraceChild =
  | { kind: 'call'; node: TraceNode }
  | { kind: 'event'; event: XdrEventDto };

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
 *  call, pop on return, and everything else becomes an event child of the
 *  call currently on top, keeping the interleaved order. `core_metrics`
 *  are host resource counters, not calls or effects.
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
      };
      if (stack.length > 0) {
        stack[stack.length - 1].children.push({ kind: 'call', node });
      } else {
        roots.push(node);
      }
      stack.push(node);
    } else if (kind === 'fn_return') {
      const node = stack.pop();
      if (node != null) node.returnValue = event.data;
    } else if (kind === 'core_metrics') {
      // Host resource counters — not part of the call story.
    } else if (stack.length > 0) {
      stack[stack.length - 1].children.push({ kind: 'event', event });
    }
    // Events outside any call (fee events etc.) stay in the Events section.
  }
  for (const node of stack) node.unfinished = true;
  return roots;
}

function childCalls(node: TraceNode): TraceNode[] {
  return node.children
    .filter(
      (child): child is Extract<TraceChild, { kind: 'call' }> =>
        child.kind === 'call'
    )
    .map((child) => child.node);
}

/** Total call count, for the section header and collapsed-branch badges. */
export function traceCallCount(nodes: readonly TraceNode[]): number {
  return nodes.reduce(
    (sum, node) => sum + 1 + traceCallCount(childCalls(node)),
    0
  );
}

function traceEventCount(nodes: readonly TraceNode[]): number {
  return nodes.reduce(
    (sum, node) =>
      sum +
      node.children.filter((child) => child.kind === 'event').length +
      traceEventCount(childCalls(node)),
    0
  );
}

/** Folded-branch badge: whatever the branch actually hides — "5 calls",
 *  "1 event", or both. A branch holding only events must not say "0 calls". */
function foldedBadgeLabel(node: TraceNode): string {
  const calls = traceCallCount(childCalls(node));
  const events = traceEventCount([node]);
  const parts: string[] = [];
  if (calls > 0) parts.push(`${calls} call${calls === 1 ? '' : 's'}`);
  if (events > 0) parts.push(`${events} event${events === 1 ? '' : 's'}`);
  return parts.join(' · ');
}

/** Row colour encodes the KIND of announcement, not the individual event:
 *  token movements / failure diagnostics / any other protocol event. Keep in
 *  sync with the legend under the trace. */
function eventCategory(label: string): {
  paletteKey: 'info' | 'error' | 'secondary';
  hint: string;
} {
  if (['transfer', 'mint', 'burn', 'clawback'].includes(label)) {
    return { paletteKey: 'info', hint: 'token movement' };
  }
  if (['error', 'log', 'host_fn_failed'].includes(label)) {
    return { paletteKey: 'error', hint: 'failure diagnostic' };
  }
  return { paletteKey: 'secondary', hint: 'protocol event' };
}

/** One argument as short inline text, or null when it wouldn't fit a row:
 *  addresses shorten to GC4Q…K7XQ form, numbers/symbols pass through and
 *  short strings render quoted (error messages). */
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
    if ((type === 'str' || type === 'string') && inner.length <= 28) {
      return `"${inner}"`;
    }
  }
  return null;
}

type ArgsSummary =
  | { kind: 'inline'; text: string }
  | { kind: 'count'; count: number };

/** Hybrid header (option C): literal args inline when every one is
 *  short and the whole list fits a row — `transfer(GC4Q…K7XQ, 13171)`;
 *  otherwise the COUNT, which the row must render visually distinct from a
 *  literal value (muted `6 args`, not a bare `6` that reads as an argument).
 *  The full typed args stay behind the per-node disclosure either way. */
function argsSummary(args: unknown): ArgsSummary {
  if (args == null) return { kind: 'inline', text: '' };
  const typed = args as TypedVal;
  if (typed.type !== 'vec' || !Array.isArray(typed.value)) {
    return { kind: 'count', count: 1 };
  }
  const parts: string[] = [];
  for (const element of typed.value) {
    const short = shortVal(element);
    if (short == null) return { kind: 'count', count: typed.value.length };
    parts.push(short);
  }
  const joined = parts.join(', ');
  return joined.length <= 40
    ? { kind: 'inline', text: joined }
    : { kind: 'count', count: typed.value.length };
}

/** Inline summary of what an event announced: the payload topics (skipping
 *  the name) plus the data scalar(s) — `transfer(GC4Q…K7XQ, CCTU…V6J7,
 *  13171)`, `error("failing with contract error", 7)`. Values that do not
 *  fit are elided with `…`; the raw event stays behind the disclosure. */
export function eventArgsText(event: XdrEventDto): string {
  const parts: string[] = [];
  let elided = false;
  for (let i = 1; i < event.topics.length; i++) {
    const short = shortVal(event.topics[i]);
    if (short == null) elided = true;
    else parts.push(short);
  }
  const data = event.data as TypedVal | null;
  if (data != null && typeof data === 'object') {
    if (data.type === 'vec' && Array.isArray(data.value)) {
      for (const element of data.value) {
        const short = shortVal(element);
        if (short == null) elided = true;
        else parts.push(short);
      }
    } else if (data.type !== 'void') {
      const short = shortVal(data);
      if (short == null) elided = true;
      else parts.push(short);
    }
  }
  let joined = parts.join(', ');
  if (joined.length > 48) {
    joined = `${joined.slice(0, 48)}…`;
    elided = false;
  }
  return `(${joined}${elided ? (joined.length > 0 ? ', …' : '…') : ''})`;
}

/** Short inline `→ value` for scalar returns; structured values stay behind
 *  the per-node disclosure. A call that returned NOTHING says `void`
 *  explicitly — silence would be ambiguous with "value too long to show".
 *  `void` is the complete truth about the call (secondary tone, like real
 *  values); only UI abbreviations like the arg count go tertiary. */
function scalarReturn(
  value: unknown
): { kind: 'value' | 'void'; text: string } | null {
  if (value == null) return null;
  let inner: unknown = value;
  const typed = value as TypedVal;
  if (typeof typed.type === 'string') {
    if (typed.type === 'void') return { kind: 'void', text: 'void' };
    if (typed.type === 'vec' || typed.type === 'map') return null;
    inner = typed.value;
  }
  if (
    typeof inner === 'string' ||
    typeof inner === 'number' ||
    typeof inner === 'boolean'
  ) {
    const text = String(inner);
    return text.length <= 24 ? { kind: 'value', text } : null;
  }
  return null;
}

/** Shared row shell: indent, guide line, vertical rhythm. */
function RowShell({ depth, children }: { depth: number; children: ReactNode }) {
  return (
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
      {children}
    </Stack>
  );
}

function DetailsToggle({
  open,
  onToggle,
}: {
  open: boolean;
  onToggle: () => void;
}) {
  return (
    <IconButton
      size="small"
      aria-label="Arguments and data"
      aria-expanded={open}
      onClick={onToggle}
      sx={{ p: 0.25, flexShrink: 0 }}
    >
      <CodeIcon
        sx={(theme) => ({
          fontSize: 14,
          color: open
            ? theme.palette.text.primary
            : theme.palette.text.tertiary,
        })}
      />
    </IconButton>
  );
}

function DetailsPanel({
  depth,
  sections,
}: {
  depth: number;
  sections: { label: string; value: unknown }[];
}) {
  return (
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
      {sections.map((section, index) => (
        <Box key={section.label}>
          <Typography
            variant="bodyXsRegular"
            sx={(theme) => ({
              color: theme.palette.text.tertiary,
              mt: index > 0 ? 1 : 0,
            })}
          >
            {section.label}
          </Typography>
          <HighlightedJson value={section.value} />
        </Box>
      ))}
    </Box>
  );
}

/** An event the call announced, as a first-class row: chronology-true
 *  sibling of sub-calls (a transfer that fired between two sub-calls sits
 *  between them). Distinguished from calls by FORM, not colour alone: dot
 *  glyph instead of a chevron slot, category-coloured name, `by EMITTER`
 *  (only when the emitter differs from the surrounding call's contract),
 *  no return arrow, never any children. */
function EventRow({
  event,
  depth,
  parentContract,
}: {
  event: XdrEventDto;
  depth: number;
  parentContract: string | null;
}) {
  const [detailsOpen, setDetailsOpen] = useState(false);
  const label = traceEventLabel(event);
  const category = eventCategory(label);
  const showEmitter =
    event.contract_id != null && event.contract_id !== parentContract;

  return (
    <>
      <RowShell depth={depth}>
        <Box
          component="span"
          aria-hidden
          sx={(theme) => ({
            width: 20,
            flexShrink: 0,
            textAlign: 'center',
            color:
              category.paletteKey === 'error'
                ? theme.palette.text.error
                : theme.palette.text.tertiary,
            fontSize: 14,
            lineHeight: 1,
          })}
        >
          •
        </Box>
        <Typography
          variant="bodyMonoSmRegular"
          title={`Event announced by this call — ${category.hint}`}
          sx={(theme) => ({
            whiteSpace: 'nowrap',
            color:
              category.paletteKey === 'error'
                ? theme.palette.text.error
                : category.paletteKey === 'info'
                ? // Same blue family as the themed Chip color="blue".
                  theme.palette.blue[theme.palette.mode === 'dark' ? 400 : 600]
                : theme.palette.text.secondary,
          })}
        >
          {label}
          <Box
            component="span"
            sx={(theme) => ({ color: theme.palette.text.secondary })}
          >
            {eventArgsText(event)}
          </Box>
        </Typography>
        {showEmitter && (
          <Typography
            variant="bodyXsRegular"
            component="span"
            title="Contract that announced this event"
            sx={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 0.5,
              whiteSpace: 'nowrap',
            }}
          >
            <Box
              component="span"
              sx={(theme) => ({ color: theme.palette.text.tertiary })}
            >
              by
            </Box>
            <IdentifierDisplay value={event.contract_id!} type="contract" />
          </Typography>
        )}
        <DetailsToggle
          open={detailsOpen}
          onToggle={() => setDetailsOpen((open) => !open)}
        />
      </RowShell>
      <Collapse in={detailsOpen} unmountOnExit>
        <DetailsPanel
          depth={depth}
          sections={[
            { label: 'topics', value: event.topics },
            { label: 'data', value: event.data },
          ]}
        />
      </Collapse>
    </>
  );
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
  const args = argsSummary(node.args);
  const calls = childCalls(node);

  return (
    <>
      <RowShell depth={depth}>
        {hasChildren ? (
          <IconButton
            size="small"
            aria-label={childrenOpen ? 'Collapse calls' : 'Expand calls'}
            aria-expanded={childrenOpen}
            onClick={() => setChildrenOpen((open) => !open)}
            sx={{ p: 0.25, flexShrink: 0 }}
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
          // flexShrink 0 is load-bearing: when the row overflows, flexbox
          // would squeeze this empty spacer to nothing and a leaf call would
          // visually lose its indent level.
          <Box sx={{ width: 20, flexShrink: 0 }} />
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
          {node.fnName}(
          {args.kind === 'inline' ? (
            // Literal values in a distinct (secondary) tone so they never
            // read as part of the function name.
            <Box
              component="span"
              sx={(theme) => ({ color: theme.palette.text.secondary })}
            >
              {args.text}
            </Box>
          ) : (
            <Box
              component="span"
              sx={(theme) => ({
                color: theme.palette.text.tertiary,
                fontStyle: 'italic',
              })}
            >
              {args.count} {args.count === 1 ? 'arg' : 'args'}
            </Box>
          )}
          )
        </Typography>
        {node.contractId != null && (
          <Typography
            variant="bodyXsRegular"
            component="span"
            title="Contract the call executed on"
            sx={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 0.5,
              whiteSpace: 'nowrap',
            }}
          >
            <Box
              component="span"
              sx={(theme) => ({ color: theme.palette.text.tertiary })}
            >
              on
            </Box>
            <IdentifierDisplay value={node.contractId} type="contract" />
          </Typography>
        )}
        {inlineReturn != null && (
          <Typography
            variant="bodyMonoSmRegular"
            sx={(theme) => ({
              color: theme.palette.text.secondary,
              fontStyle: inlineReturn.kind === 'void' ? 'italic' : 'normal',
              whiteSpace: 'nowrap',
            })}
          >
            → {inlineReturn.text}
          </Typography>
        )}
        {hasChildren && !childrenOpen && (
          <Chip size="sm" color="neutral" label={foldedBadgeLabel(node)} />
        )}
        {/* The whole unfinished stack path is marked in the model, but the
            chip renders only at the DEEPEST unfinished call — repeating it
            on every ancestor reads as noise (review finding); the nesting
            already shows the path. */}
        {node.unfinished && !calls.some((call) => call.unfinished) && (
          <Chip size="sm" color="error" label="stopped here" />
        )}
        {hasDetails && (
          <DetailsToggle
            open={detailsOpen}
            onToggle={() => setDetailsOpen((open) => !open)}
          />
        )}
      </RowShell>
      {hasDetails && (
        <Collapse in={detailsOpen} unmountOnExit>
          <DetailsPanel
            depth={depth}
            sections={[
              ...(node.args != null
                ? [{ label: 'arguments', value: node.args }]
                : []),
              ...(node.returnValue != null
                ? [{ label: 'return', value: node.returnValue }]
                : []),
            ]}
          />
        </Collapse>
      )}
      <Collapse in={childrenOpen} unmountOnExit>
        {node.children.map((child, index) =>
          child.kind === 'call' ? (
            <TraceNodeRow key={index} node={child.node} depth={depth + 1} />
          ) : (
            <EventRow
              key={index}
              event={child.event}
              depth={depth + 1}
              parentContract={node.contractId}
            />
          )
        )}
      </Collapse>
    </>
  );
}

function hasEventChildren(nodes: readonly TraceNode[]): boolean {
  return nodes.some((node) =>
    node.children.some(
      (child) =>
        child.kind === 'event' ||
        (child.kind === 'call' && hasEventChildren([child.node]))
    )
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
      {/* Legend for the event-row colour categories (see eventCategory). */}
      {hasEventChildren(nodes) && (
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({ color: theme.palette.text.tertiary, mt: 0.5 })}
        >
          • rows are events announced by the surrounding call — blue: token
          movement · grey: protocol event · red: failure diagnostics.
        </Typography>
      )}
    </Box>
  );
}
