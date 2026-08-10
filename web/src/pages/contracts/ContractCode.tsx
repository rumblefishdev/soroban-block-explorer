import BugReportOutlinedIcon from '@mui/icons-material/BugReportOutlined';
import FileDownloadOutlinedIcon from '@mui/icons-material/FileDownloadOutlined';
import InfoOutlinedIcon from '@mui/icons-material/InfoOutlined';
import {
  Alert,
  Box,
  Button,
  Link,
  Stack,
  ToggleButton,
  ToggleButtonGroup,
  Tooltip,
  Typography,
} from '@mui/material';
import {
  CardSkeleton,
  Chip,
  CopyButton,
  QueryErrorState,
} from '@rumblefish/soroban-block-explorer-ui';
import { Suspense, lazy, useEffect, useState } from 'react';

import { useContractDecompiled } from '../../api/index.js';

import type { DecompileDiagnostic } from '@rumblefish/api-types';

// Prism (and its grammars) load in their own chunk on first Code-tab use.
const CodeHighlight = lazy(() => import('./CodeHighlight.js'));

const SOROBAN_RET_REPO = 'https://github.com/Inferara/soroban-ret';

/** Prefilled GitHub issue against the decompiler repo (task 0465; the
 *  final issue-form template is pending upstream — URL prefill until then).
 *  Also used from the error state, where the API error message is the most
 *  valuable part of the report. */
function reportIssueUrl(opts: {
  contractId: string;
  wasmHash: string;
  /** Decompiler version; always sent — it is the field that tells the
   *  maintainers which build to reproduce against. */
  version: string;
  /** The representation the user asked for. */
  requested: 'rust' | 'wat';
  /** What ended up on screen; absent when nothing could be rendered. */
  shown?: string;
  /** Failure detail, already attributed to whoever produced it. */
  apiError?: string;
}): string {
  const title = `Decompilation issue: ${opts.contractId}`;
  const body = [
    `- Contract: \`${opts.contractId}\``,
    `- WASM hash: \`${opts.wasmHash}\``,
    `- soroban-ret version: ${opts.version}`,
    `- Requested: ${opts.requested}`,
    `- Rendered: ${opts.shown ?? 'nothing (request failed)'}`,
    ...(opts.apiError ? [`- Failure: ${opts.apiError}`] : []),
    `- Seen on: https://sorobanscan.rumblefish.dev/contracts/${opts.contractId}?tab=code`,
    '',
    'What looks wrong:',
    '',
  ].join('\n');
  return `${SOROBAN_RET_REPO}/issues/new?title=${encodeURIComponent(
    title
  )}&body=${encodeURIComponent(body)}`;
}

const plural = (n: number, noun: string) => `${n} ${noun}${n === 1 ? '' : 's'}`;

/**
 * Version reported when the request failed before any payload arrived. The
 * endpoint pins the decompiler, so the constant is accurate — it just
 * cannot be read off a response that never came.
 */
const SOROBAN_RET_VERSION_FALLBACK = '0.0.4 (pinned by the API)';

/**
 * Attribute a failure before it is sent upstream. `decompile_failed` on a
 * timeout is SorobanScan's own limit, not a soroban-ret diagnostic —
 * forwarding its wording verbatim would blame the decompiler for our knife
 * and ship UI advice ("retry with format=wat") into their tracker.
 */
function attributedFailure(
  message: string | undefined,
  code: string | undefined
): string | undefined {
  if (code !== 'decompile_failed') return message;
  if (message?.includes('timed out')) {
    return (
      "exceeded SorobanScan's 10 s server-side decompilation limit " +
      '(no soroban-ret error — the decompiler was still running)'
    );
  }
  return message;
}

/**
 * `call_indirect` warnings are almost always benign `core::fmt` vtables —
 * per the soroban-ret team, they must not read as errors. They stay
 * listed (they do explain thin spots) but in the muted, informational
 * style rather than the warning one.
 */
const BENIGN_CATEGORIES = new Set(['call_indirect']);

/** Human label for a diagnostic category slug. */
function categoryLabel(category: string): string {
  return category.replace(/_/g, ' ');
}

/**
 * Collapse diagnostics to one row per (category, message) with an
 * occurrence count and the functions involved. The decompiler emits one
 * entry per offending instruction — a real mainnet contract yields 19
 * identical `call_indirect` warnings across 5 functions, which as a flat
 * list is noise rather than information.
 */
function groupDiagnostics(diagnostics: DecompileDiagnostic[]) {
  const groups = new Map<
    string,
    {
      category: string;
      severity: string;
      message: string;
      count: number;
      functions: Set<number>;
    }
  >();
  for (const d of diagnostics) {
    const key = `${d.category} ${d.message}`;
    const existing = groups.get(key);
    const group = existing ?? {
      category: d.category,
      severity: d.severity,
      message: d.message,
      count: 0,
      functions: new Set<number>(),
    };
    group.count += 1;
    if (d.function_index != null) group.functions.add(d.function_index);
    // A category is only as benign as its worst entry.
    if (d.severity === 'warning') group.severity = 'warning';
    if (existing == null) groups.set(key, group);
  }
  return [...groups.values()];
}

function downloadSource(
  contractId: string,
  representation: string,
  source: string
) {
  const blob = new Blob([source], { type: 'text/plain' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `${contractId}.${representation === 'rust' ? 'rs' : 'wat'}`;
  a.click();
  URL.revokeObjectURL(url);
}

/**
 * Error-envelope `code` from a failed query, when the API sent one.
 * The api-client interceptor (`web/src/api/client.ts`) re-wraps every
 * failure into a real `Error` and preserves the ADR-0008 envelope under
 * `.body`, so that is where `code` lives; network failures have none.
 */
function errorCode(error: unknown): string | undefined {
  if (typeof error !== 'object' || error == null) return undefined;
  const body = (error as { body?: unknown }).body;
  const source = typeof body === 'object' && body != null ? body : error;
  const code = (source as { code?: unknown }).code;
  return typeof code === 'string' ? code : undefined;
}

/** The interceptor adopts the envelope `message` as `Error.message`. */
function errorMessage(error: unknown): string | undefined {
  if (typeof error === 'object' && error != null && 'message' in error) {
    const message = (error as { message?: unknown }).message;
    return typeof message === 'string' ? message : undefined;
  }
  return undefined;
}

/**
 * Code tab (task 0465, issue #374) — the contract's WASM decompiled on
 * demand by soroban-ret. Experimental by design: the banner and the
 * completeness counters stay visible whatever the fidelity, and unrecovered
 * values render as explicit `todo!()` holes in the source itself.
 *
 * Fallback ladder inside the tab, in order:
 * 1. Rust — the default request.
 * 2. WAT delivered in-response (`representation: "wat"` + `rust_error`)
 *    when Rust *emission* fails inside a successful call.
 * 3. WAT re-requested automatically when the Rust call itself dies with
 *    `decompile_failed` (timeout / no representation) — the API's own
 *    hint is "retry with format=wat", so the UI just does it. The user
 *    sees code plus a "WAT only" chip, not an error wall.
 * 4. Error state with the toolbar still mounted (the Rust/WAT toggle must
 *    not disappear with the content) and, for decompiler-side failures,
 *    a "Report issue" action prefilled with the API error.
 *
 * The tab itself is only mounted for contracts with a WASM (the page hides
 * it for SAC / pre-upload), so a 404 here is unexpected and surfaces as
 * the generic error state.
 */
export function ContractCode({
  contractId,
  wasmHash,
}: {
  contractId: string;
  wasmHash: string;
}) {
  const [format, setFormat] = useState<'rust' | 'wat'>('rust');
  // Set when the Rust call failed hard and the UI fell back to WAT on its
  // own; carries the API error message for the "WAT only" chip tooltip.
  const [autoWatReason, setAutoWatReason] = useState<string | null>(null);
  const { data, isPending, isError, error, refetch, fetchStatus } =
    useContractDecompiled(contractId, format);

  const failedCode = isError ? errorCode(error) : undefined;

  /**
   * Evidence that Rust cannot be produced for THIS contract: either the
   * call died (`autoWatReason`) or it succeeded but carried WAT instead
   * (`rust_error`). Drives the disabled Rust toggle — a user who picks WAT
   * on a healthy contract keeps Rust available, because `rust_error` is
   * null there.
   */
  const rustUnavailable: { reason: string; fromDecompiler: boolean } | null =
    autoWatReason != null
      ? { reason: autoWatReason, fromDecompiler: false }
      : data?.representation === 'wat' && data.rust_error != null
      ? { reason: data.rust_error, fromDecompiler: true }
      : null;

  /**
   * What the prefilled issue should say. soroban-ret's own message goes in
   * verbatim; our timeout is reported as ours, because from the
   * decompiler's side nothing failed — it simply had not finished.
   */
  const reportedError =
    rustUnavailable == null
      ? undefined
      : rustUnavailable.fromDecompiler
      ? rustUnavailable.reason
      : "exceeded SorobanScan's 10 s server-side decompilation limit " +
        '(no soroban-ret error — the decompiler was still running)';

  /** The toggle reflects what is on screen, not what was requested — those
   *  differ when the API answers a Rust request with the WAT fallback. */
  const shown = data?.representation ?? format;

  // Ladder step 3: a dead Rust call degrades to WAT automatically. Only for
  // `decompile_failed` — network/API errors would fail on WAT too, so they
  // stay in the error state instead of doubling the noise.
  useEffect(() => {
    if (format === 'rust' && failedCode === 'decompile_failed') {
      setAutoWatReason(errorMessage(error) ?? 'Rust decompilation failed');
      setFormat('wat');
    }
  }, [format, failedCode, error]);

  const toolbar = (
    <Stack
      direction="row"
      spacing={1}
      alignItems="center"
      sx={{ flexWrap: 'wrap', rowGap: 1 }}
    >
      <ToggleButtonGroup
        exclusive
        size="small"
        value={shown}
        onChange={(_e, next: 'rust' | 'wat' | null) => {
          if (next != null) {
            setAutoWatReason(null);
            setFormat(next);
          }
        }}
        aria-label="Source representation"
        sx={{ '& .MuiToggleButton-root': { textTransform: 'none' } }}
      >
        {/* Disabled when this contract has no Rust representation at all —
            better a dead option than a Rust tab quietly showing WAT. The
            reason lives on the "WAT only" chip, which stays hoverable
            (a disabled button swallows mouse events). */}
        <ToggleButton value="rust" disabled={rustUnavailable != null}>
          Rust
        </ToggleButton>
        <ToggleButton value="wat">WAT</ToggleButton>
      </ToggleButtonGroup>
      <Chip size="sm" color="warning" label="Experimental" />
      {data != null && (
        <Chip
          size="sm"
          color="neutral"
          label={`soroban-ret ${data.soroban_ret_version}`}
        />
      )}
      {data?.sdk_version != null && (
        // contractmetav0 stores "<version>#<git sha>"; the sha is 40 chars
        // of noise in a toolbar chip, so it moves to the tooltip.
        <Tooltip title={data.sdk_version} arrow>
          <Chip
            size="sm"
            color="neutral"
            label={`SDK ${data.sdk_version.split('#')[0]}`}
          />
        </Tooltip>
      )}
      {rustUnavailable != null && (
        // Rust could not be produced for this contract — emission failed
        // inside a successful call, or the call itself died and the UI
        // re-requested WAT. The full reason renders in the alert below;
        // the tooltip is a shortcut for people hovering the chip.
        <Tooltip title={rustUnavailable.reason} arrow>
          <Chip size="sm" color="warning" label="WAT only" />
        </Tooltip>
      )}
      <Box sx={{ flexGrow: 1 }} />
      {/* One report CTA at a time: when Rust is unavailable the alert below
          carries it, next to the reason it should quote. */}
      {data != null && rustUnavailable == null && (
        <>
          <Button
            size="small"
            color="inherit"
            startIcon={<BugReportOutlinedIcon />}
            href={reportIssueUrl({
              contractId,
              wasmHash: data.wasm_hash,
              version: data.soroban_ret_version,
              requested: format,
              shown: data.representation,
            })}
            target="_blank"
            rel="noopener noreferrer"
          >
            Report issue
          </Button>
        </>
      )}
      {data != null && (
        <Button
          size="small"
          color="inherit"
          startIcon={<FileDownloadOutlinedIcon />}
          onClick={() =>
            downloadSource(contractId, data.representation, data.source)
          }
        >
          {/* Labelled with the exact file the click produces — the active
              representation (covers the WAT-fallback case too). */}
          Download {data.representation === 'rust' ? '.rs' : '.wat'}
        </Button>
      )}
    </Stack>
  );

  // `isPending`, not `isLoading`: between retry attempts the query is
  // pending but not fetching, and `isLoading` goes false there — which
  // flashed the error state mid-retry on slow contracts.
  // A paused fetch never resolves on its own — TanStack pauses queries when
  // its online manager reports the app offline (observed with the API host
  // unreachable), and the skeleton below would then spin forever. Treat it
  // as an error state so there is always a way out.
  const stalled = fetchStatus === 'paused';

  if (isPending && !stalled) {
    return (
      <Stack spacing={1.5} sx={{ p: 2 }}>
        {toolbar}
        <CardSkeleton />
      </Stack>
    );
  }

  if (isError || stalled || data == null) {
    // Ladder step 4. `decompile_failed` here means even the WAT path (or a
    // direct WAT request) failed — a genuine decompiler-side case worth a
    // prefilled report. Other errors (network, API down, rate limit) are
    // not soroban-ret's fault, so no report button for those.
    return (
      <Stack spacing={1.5} sx={{ p: 2 }}>
        {toolbar}
        <QueryErrorState error={error} onRetry={() => void refetch()} />
        {failedCode === 'decompile_failed' && (
          <Box sx={{ textAlign: 'center' }}>
            <Button
              size="small"
              color="inherit"
              startIcon={<BugReportOutlinedIcon />}
              href={reportIssueUrl({
                contractId,
                wasmHash,
                version: SOROBAN_RET_VERSION_FALLBACK,
                requested: format,
                apiError: attributedFailure(errorMessage(error), failedCode),
              })}
              target="_blank"
              rel="noopener noreferrer"
            >
              Report issue to soroban-ret
            </Button>
          </Box>
        )}
      </Stack>
    );
  }

  const isRust = data.representation === 'rust';

  return (
    <Stack spacing={1.5} sx={{ p: 2 }}>
      {toolbar}

      <Alert severity="warning" icon={<InfoOutlinedIcon fontSize="small" />}>
        Automatically reconstructed from the on-chain WASM — not verified
        source. Signatures and types come from the contract&apos;s own metadata;
        function bodies are inferred and may be incomplete. Unrecovered values
        appear as <code>todo!()</code>.
      </Alert>

      {rustUnavailable != null && (
        // The reason in the open — a hidden tooltip undersells exactly the
        // case worth reporting upstream. Attribution matters: a decompiler
        // diagnostic is quoted verbatim, our own timeout is named as ours.
        <Alert
          severity="warning"
          icon={<BugReportOutlinedIcon fontSize="small" />}
          action={
            <Button
              color="inherit"
              size="small"
              href={reportIssueUrl({
                contractId,
                wasmHash,
                version: data.soroban_ret_version,
                requested: 'rust',
                shown: data.representation,
                apiError: reportedError,
              })}
              target="_blank"
              rel="noopener noreferrer"
            >
              Report issue
            </Button>
          }
        >
          {rustUnavailable.fromDecompiler ? (
            <>
              Rust could not be produced for this contract:{' '}
              <code>{rustUnavailable.reason}</code>
            </>
          ) : (
            <>
              Rust decompilation exceeded SorobanScan&apos;s 10-second limit, so
              WAT is shown instead — soroban-ret reported no error, it was still
              running.
            </>
          )}
        </Alert>
      )}

      {isRust && (
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({ color: theme.palette.text.tertiary })}
        >
          {plural(data.functions ?? 0, 'function')} ·{' '}
          {plural(data.todo_holes ?? 0, 'unresolved hole')} ·{' '}
          {plural(data.unknown_vars ?? 0, 'unknown value')}
        </Typography>
      )}

      {data.diagnostics.length > 0 && (
        // Why a reconstruction may be thin: constructs soroban-ret does not
        // model well. Listed, not summarised — a count would say nothing
        // actionable, and the messages are short and specific.
        <Stack
          spacing={0.5}
          sx={(theme) => ({
            p: 1.5,
            borderRadius: `${theme.shape.radius.s}px`,
            border: `1px solid ${theme.palette.stroke.default}`,
          })}
        >
          <Typography
            variant="bodyXsMedium"
            sx={(theme) => ({ color: theme.palette.text.secondary })}
          >
            Binary features affecting decompilation
          </Typography>
          {groupDiagnostics(data.diagnostics).map((d) => {
            const benign =
              BENIGN_CATEGORIES.has(d.category) || d.severity === 'info';
            const fns = [...d.functions].sort((a, b) => a - b);
            return (
              <Stack
                key={`${d.category}-${d.message}`}
                direction="row"
                spacing={1}
                alignItems="baseline"
              >
                <Chip
                  size="sm"
                  color={benign ? 'neutral' : 'warning'}
                  label={categoryLabel(d.category)}
                />
                <Typography
                  variant="bodyXsRegular"
                  sx={(theme) => ({ color: theme.palette.text.tertiary })}
                >
                  {d.message}
                  {d.count > 1 && ` · ${d.count}×`}
                  {fns.length > 0 &&
                    ` · ${plural(fns.length, 'function')} (${fns
                      .slice(0, 5)
                      .map((n) => `#${n}`)
                      .join(', ')}${fns.length > 5 ? '…' : ''})`}
                </Typography>
              </Stack>
            );
          })}
        </Stack>
      )}

      <Box sx={{ position: 'relative' }}>
        <Box sx={{ position: 'absolute', top: 8, right: 12, zIndex: 2 }}>
          <CopyButton value={data.source} ariaLabel="Copy source" />
        </Box>
        <Suspense
          fallback={
            <Box
              component="pre"
              sx={(theme) => ({
                m: 0,
                p: 2,
                borderRadius: `${theme.shape.radius.s}px`,
                border: `1px solid ${theme.palette.stroke.default}`,
                backgroundColor: theme.palette.surface.grayMainAlt,
                overflow: 'auto',
                maxHeight: 640,
                fontFamily: 'monospace',
                fontSize: 13,
                lineHeight: '21px',
              })}
            >
              {data.source}
            </Box>
          }
        >
          <CodeHighlight
            source={data.source}
            language={isRust ? 'rust' : 'wasm'}
          />
        </Suspense>
      </Box>

      <Typography
        variant="bodyXsRegular"
        sx={(theme) => ({ color: theme.palette.text.tertiary })}
      >
        WASM decompilation provided by Inferara{' '}
        <Link
          href={SOROBAN_RET_REPO}
          target="_blank"
          rel="noopener noreferrer"
          color="inherit"
        >
          soroban-ret
        </Link>{' '}
        ·{' '}
        <Link
          href="https://inferara.com/"
          target="_blank"
          rel="noopener noreferrer"
          color="inherit"
        >
          inferara.com
        </Link>
      </Typography>
    </Stack>
  );
}
