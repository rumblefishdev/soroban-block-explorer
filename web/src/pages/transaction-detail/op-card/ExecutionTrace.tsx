import CodeIcon from '@mui/icons-material/Code';
import ExpandMoreIcon from '@mui/icons-material/ExpandMore';
import { Box, Collapse, IconButton, Stack, Typography } from '@mui/material';
import { alpha } from '@mui/material/styles';
import type { Theme } from '@mui/material/styles';
import { Chip, IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
import { Fragment, useState } from 'react';

import type { XdrEventDto } from '@rumblefish/api-types';

import { HighlightedJson } from './HighlightedJson.js';
import { STRKEY_VERSION, strkeyFromBase64 } from './strkeyDecode.js';

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

/** Contract strkey from the fn_call bytes topic — see strkeyDecode.ts. */
export function contractStrkeyFromBase64(b64: string): string | null {
  return strkeyFromBase64(b64, STRKEY_VERSION.contract);
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
        // The host always names the function in the third topic; a missing
        // name means malformed data, and the display must SAY so — a
        // plausible generic like `call` would read as a real fn name.
        fnName: symTopic(event, 2) ?? '(unknown)',
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

type EventPaletteKey = 'info' | 'error' | 'secondary';

/** Per-category event counts for a folded branch — the badge must break
 *  events down 1:1 with the row colours, not lump them into one number. */
function traceEventCountByCategory(
  nodes: readonly TraceNode[]
): Record<EventPaletteKey, number> {
  const counts: Record<EventPaletteKey, number> = {
    info: 0,
    secondary: 0,
    error: 0,
  };
  const walk = (list: readonly TraceNode[]) => {
    for (const node of list) {
      for (const child of node.children) {
        if (child.kind === 'event') {
          counts[eventCategory(traceEventLabel(child.event)).paletteKey] += 1;
        }
      }
      walk(childCalls(node));
    }
  };
  walk(nodes);
  return counts;
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
  return { paletteKey: 'secondary', hint: 'contract event' };
}

/** One inline piece of a value list: plain text, or an ADDRESS that must
 *  render as the house identifier link (review requirement: every address on the page is
 *  clickable). */
export type InlinePart =
  | { kind: 'text'; text: string }
  | { kind: 'id'; value: string; short: string };

function idType(value: string): 'contract' | 'account' | 'pool' {
  if (value.startsWith('C')) return 'contract';
  if (value.startsWith('L')) return 'pool';
  return 'account';
}

/** One argument as an inline part, or null when it wouldn't fit a row:
 *  addresses become linkable parts (shortened GC4Q…K7XQ), numbers/symbols
 *  pass through and short strings render quoted (error messages). */
function shortPart(value: unknown): InlinePart | null {
  if (value == null || typeof value !== 'object') return null;
  const { type, value: inner } = value as TypedVal;
  if (type === 'address' && typeof inner === 'string' && inner.length > 12) {
    return {
      kind: 'id',
      value: inner,
      short: `${inner.slice(0, 4)}…${inner.slice(-4)}`,
    };
  }
  if (typeof inner === 'number' || typeof inner === 'boolean') {
    return { kind: 'text', text: String(inner) };
  }
  if (typeof inner === 'string') {
    // Big ints (i128/u128…) arrive as decimal strings; symbols stay short.
    if (/^-?\d+$/.test(inner)) return { kind: 'text', text: inner };
    if (type === 'sym' && inner.length <= 12) {
      return { kind: 'text', text: inner };
    }
    if ((type === 'str' || type === 'string') && inner.length <= 28) {
      return { kind: 'text', text: `"${inner}"` };
    }
  }
  return null;
}

function partLength(part: InlinePart): number {
  return part.kind === 'text' ? part.text.length : part.short.length;
}

function partsLength(parts: readonly InlinePart[]): number {
  return (
    parts.reduce((sum, part) => sum + partLength(part), 0) +
    Math.max(0, parts.length - 1) * 2
  );
}

/** Plain-text form of an inline part list (tests + length budget). */
export function partsToText(parts: readonly InlinePart[]): string {
  return parts
    .map((part) => (part.kind === 'text' ? part.text : part.short))
    .join(', ');
}

type ArgsSummary =
  | { kind: 'inline'; parts: InlinePart[] }
  | { kind: 'count'; count: number };

/** Hybrid header (option C): literal args inline when every one is
 *  short and the whole list fits a row — `transfer(GC4Q…K7XQ, 13171)`;
 *  otherwise the COUNT, which the row must render visually distinct from a
 *  literal value (muted `6 args`, not a bare `6` that reads as an argument).
 *  A single scalar argument (the host does not wrap one-arg calls in a vec)
 *  inlines the same way — `balance(CB3E…KXWE)`, not `balance(1 arg)`.
 *  The full typed args stay behind the per-node disclosure either way. */
export function argsSummary(args: unknown): ArgsSummary {
  if (args == null) return { kind: 'inline', parts: [] };
  const typed = args as TypedVal;
  // A void payload means NO arguments — it must render as `fn()`, never
  // fall through to the "1 arg" count (a plausible-but-wrong display).
  if (typed.type === 'void') return { kind: 'inline', parts: [] };
  if (typed.type !== 'vec' || !Array.isArray(typed.value)) {
    const single = shortPart(args);
    return single != null && partLength(single) <= 40
      ? { kind: 'inline', parts: [single] }
      : { kind: 'count', count: 1 };
  }
  const parts: InlinePart[] = [];
  for (const element of typed.value) {
    const part = shortPart(element);
    if (part == null) return { kind: 'count', count: typed.value.length };
    parts.push(part);
  }
  return partsLength(parts) <= 40
    ? { kind: 'inline', parts }
    : { kind: 'count', count: typed.value.length };
}

/** Inline summary of what an event announced: the payload topics (skipping
 *  the name) plus the data scalar(s) — `transfer(GC4Q…K7XQ, CCTU…V6J7,
 *  13171)`, `error("failing with contract error", 7)`. Values that do not
 *  fit are elided with `…`; the raw event stays behind the disclosure. */
export function eventArgsParts(event: XdrEventDto): InlinePart[] {
  const parts: InlinePart[] = [];
  let elided = false;
  const push = (value: unknown) => {
    const part = shortPart(value);
    if (part == null) elided = true;
    else parts.push(part);
  };
  for (let i = 1; i < event.topics.length; i++) push(event.topics[i]);
  const data = event.data as TypedVal | null;
  if (data != null && typeof data === 'object') {
    if (data.type === 'vec' && Array.isArray(data.value)) {
      for (const element of data.value) push(element);
    } else if (data.type !== 'void') {
      push(data);
    }
  }
  while (partsLength(parts) > 48 && parts.length > 0) {
    parts.pop();
    elided = true;
  }
  if (elided) parts.push({ kind: 'text', text: '…' });
  return parts;
}

/** Plain-text form of the event payload summary (kept for tests). */
export function eventArgsText(event: XdrEventDto): string {
  return `(${partsToText(eventArgsParts(event))})`;
}

/** Short inline `→ value` for scalar returns; structured values stay behind
 *  the per-node disclosure. A call that returned NOTHING says `void`
 *  explicitly — silence would be ambiguous with "value too long to show".
 *  `void` is the complete truth about the call (secondary tone, like real
 *  values); only UI abbreviations go tertiary. A vec of short values (e.g.
 *  a pair of token addresses from `get_tokens`) inlines with links, exactly
 *  like arguments do. A return that exists but cannot inline (struct, long
 *  value) shows `→ …` — silence would be indistinguishable from "returned
 *  nothing" (silent-fallback audit). */
type ReturnSummary =
  | { kind: 'value'; parts: InlinePart[] }
  | { kind: 'void' }
  | { kind: 'count'; text: string };

function returnSummary(value: unknown): ReturnSummary | null {
  if (value == null) return null;
  const typed = value as TypedVal;
  if (typed.type === 'void') return { kind: 'void' };
  if (typed.type === 'vec' && Array.isArray(typed.value)) {
    const count = {
      kind: 'count' as const,
      text: `${typed.value.length} value${typed.value.length === 1 ? '' : 's'}`,
    };
    const parts: InlinePart[] = [];
    for (const element of typed.value) {
      const part = shortPart(element);
      if (part == null) return count;
      parts.push(part);
    }
    return partsLength(parts) <= 40 ? { kind: 'value', parts } : count;
  }
  const single = shortPart(value);
  return single != null && partLength(single) <= 24
    ? { kind: 'value', parts: [single] }
    : { kind: 'count', text: '1 value' };
}

/** Inline value list where every address is the house identifier link
 *  (default truncation), inheriting the surrounding tone and font size. */
function InlineParts({ parts }: { parts: readonly InlinePart[] }) {
  return (
    <>
      {parts.map((part, index) => (
        <Fragment key={index}>
          {index > 0 && ', '}
          {part.kind === 'text' ? (
            part.text
          ) : (
            <IdentifierDisplay
              value={part.value}
              type={idType(part.value)}
              tone="inherit"
              fontSize="inherit"
            />
          )}
        </Fragment>
      ))}
    </>
  );
}

function eventColor(paletteKey: EventPaletteKey, theme: Theme): string {
  if (paletteKey === 'error') return theme.palette.text.error;
  if (paletteKey === 'info') {
    return theme.palette.blue[theme.palette.mode === 'dark' ? 400 : 600];
  }
  return theme.palette.mode === 'dark'
    ? theme.palette.stroke.warning
    : theme.palette.text.warning;
}

const CATEGORY_ORDER: EventPaletteKey[] = ['info', 'secondary', 'error'];

// Short badge words — must match the legend and eventCategory hints.
const CATEGORY_WORD: Record<EventPaletteKey, string> = {
  info: 'token',
  secondary: 'contract',
  error: 'error',
};

/** Folded-branch badge: the calls count plus a colour-matched dot count per
 *  event category — 1:1 with the row colours, never one aggregate number. */
function FoldedBadge({ node }: { node: TraceNode }) {
  const calls = traceCallCount(childCalls(node));
  const events = traceEventCountByCategory([node]);
  return (
    <Box
      component="span"
      sx={(theme) => ({
        display: 'inline-flex',
        alignItems: 'center',
        gap: 0.75,
        px: 1,
        py: 0.25,
        borderRadius: 999,
        border: `1px solid ${theme.palette.stroke.default}`,
        color: theme.palette.text.secondary,
        fontSize: 11,
        whiteSpace: 'nowrap',
        flexShrink: 0,
      })}
    >
      {calls > 0 && (
        <span>
          {calls} call{calls === 1 ? '' : 's'}
        </span>
      )}
      {CATEGORY_ORDER.filter((key) => events[key] > 0).map((key) => (
        <Box
          key={key}
          component="span"
          sx={(theme) => ({
            display: 'inline-flex',
            alignItems: 'center',
            gap: 0.25,
            color: eventColor(key, theme),
          })}
        >
          •{events[key]} {CATEGORY_WORD[key]}
        </Box>
      ))}
    </Box>
  );
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
 *  between them). Distinguished from calls by FORM and colour: dot glyph
 *  instead of a chevron slot, category-coloured name, `by EMITTER` always
 *  shown (mirrors the calls' `on CALLEE`; hiding it when it matched the
 *  surrounding call read as an omission), no return arrow, never
 *  any children. */
function EventRow({ event, depth }: { event: XdrEventDto; depth: number }) {
  const [detailsOpen, setDetailsOpen] = useState(false);
  const label = traceEventLabel(event);
  const category = eventCategory(label);

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
              category.paletteKey === 'secondary'
                ? theme.palette.text.tertiary
                : eventColor(category.paletteKey, theme),
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
            color: eventColor(category.paletteKey, theme),
          })}
        >
          {/* Tinted pill behind the NAME: shape separates events from call
              rows even where hue alone is not enough (contrast review). */}
          <Box
            component="span"
            sx={(theme) => ({
              px: 0.75,
              py: 0.125,
              borderRadius: `${theme.shape.radius.s}px`,
              backgroundColor: alpha(
                eventColor(category.paletteKey, theme),
                theme.palette.mode === 'dark' ? 0.16 : 0.12
              ),
            })}
          >
            {label}
          </Box>
          <Box
            component="span"
            sx={(theme) => ({ color: theme.palette.text.secondary })}
          >
            (<InlineParts parts={eventArgsParts(event)} />)
          </Box>
        </Typography>
        {event.contract_id != null && (
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
  const inlineReturn = returnSummary(node.returnValue);
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
          // Every row carries a marker (event rows have their •): leaf calls
          // get a muted ƒ so no row starts with a bare gap. flexShrink 0 is
          // load-bearing: when the row overflows, flexbox would squeeze this
          // spacer to nothing and a leaf call would visually lose its indent.
          <Box
            component="span"
            aria-hidden
            sx={(theme) => ({
              width: 20,
              flexShrink: 0,
              textAlign: 'center',
              color: theme.palette.text.tertiary,
              fontSize: 13,
              lineHeight: 1,
            })}
          >
            ƒ
          </Box>
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
              <InlineParts parts={args.parts} />
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
              // opaque `…` is a UI abbreviation → tertiary, like the arg
              // count; void and real values are the truth → secondary.
              color:
                inlineReturn.kind === 'count'
                  ? theme.palette.text.tertiary
                  : theme.palette.text.secondary,
              fontStyle: inlineReturn.kind === 'value' ? 'normal' : 'italic',
              whiteSpace: 'nowrap',
            })}
          >
            →{' '}
            {inlineReturn.kind === 'value' ? (
              <InlineParts parts={inlineReturn.parts} />
            ) : inlineReturn.kind === 'void' ? (
              'void'
            ) : (
              inlineReturn.text
            )}
          </Typography>
        )}
        {hasChildren && !childrenOpen && <FoldedBadge node={node} />}
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
            <EventRow key={index} event={child.event} depth={depth + 1} />
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
          movement · amber: contract event · red: failure diagnostics.
        </Typography>
      )}
    </Box>
  );
}
