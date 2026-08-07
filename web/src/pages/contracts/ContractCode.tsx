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
  Typography,
} from '@mui/material';
import {
  CardSkeleton,
  Chip,
  CopyButton,
  QueryErrorState,
} from '@rumblefish/soroban-block-explorer-ui';
import { Suspense, lazy, useState } from 'react';

import { useContractDecompiled } from '../../api/index.js';

// Prism (and its grammars) load in their own chunk on first Code-tab use.
const CodeHighlight = lazy(() => import('./CodeHighlight.js'));

const SOROBAN_RET_REPO = 'https://github.com/Inferara/soroban-ret';

/** Prefilled GitHub issue against the decompiler repo (task 0465; the
 *  final issue-form template is pending upstream — URL prefill until then). */
function reportIssueUrl(
  contractId: string,
  wasmHash: string,
  version: string,
  representation: string
): string {
  const title = `Decompilation issue: ${contractId}`;
  const body = [
    `- Contract: \`${contractId}\``,
    `- WASM hash: \`${wasmHash}\``,
    `- soroban-ret version: ${version}`,
    `- Representation shown: ${representation}`,
    `- Seen on: https://sorobanscan.rumblefish.dev/contracts/${contractId}?tab=code`,
    '',
    'What looks wrong:',
    '',
  ].join('\n');
  return `${SOROBAN_RET_REPO}/issues/new?title=${encodeURIComponent(
    title
  )}&body=${encodeURIComponent(body)}`;
}

const plural = (n: number, noun: string) => `${n} ${noun}${n === 1 ? '' : 's'}`;

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
 * Code tab (task 0465, issue #374) — the contract's WASM decompiled on
 * demand by soroban-ret. Experimental by design: the banner and the
 * completeness counters stay visible whatever the fidelity, and unrecovered
 * values render as explicit `todo!()` holes in the source itself.
 *
 * Fallback ladder inside the tab: Rust → WAT-in-response fallback
 * (`representation: "wat"` + `rust_error`) → error state with retry. The
 * tab itself is only mounted for contracts with a WASM (the page hides it
 * for SAC / pre-upload), so a 404 here is unexpected and surfaces as the
 * generic error state.
 */
export function ContractCode({ contractId }: { contractId: string }) {
  const [format, setFormat] = useState<'rust' | 'wat'>('rust');
  const { data, isLoading, isError, error, refetch } = useContractDecompiled(
    contractId,
    format
  );

  if (isLoading) {
    return (
      <Box sx={{ p: 2 }}>
        <CardSkeleton />
      </Box>
    );
  }

  if (isError || data == null) {
    return <QueryErrorState error={error} onRetry={() => void refetch()} />;
  }

  const isRust = data.representation === 'rust';
  const watFallback = format === 'rust' && data.representation === 'wat';

  return (
    <Stack spacing={1.5} sx={{ p: 2 }}>
      <Stack
        direction="row"
        spacing={1}
        alignItems="center"
        sx={{ flexWrap: 'wrap', rowGap: 1 }}
      >
        <ToggleButtonGroup
          exclusive
          size="small"
          value={format}
          onChange={(_e, next: 'rust' | 'wat' | null) => {
            if (next != null) setFormat(next);
          }}
          aria-label="Source representation"
          sx={{ '& .MuiToggleButton-root': { textTransform: 'none' } }}
        >
          <ToggleButton value="rust">Rust</ToggleButton>
          <ToggleButton value="wat">WAT</ToggleButton>
        </ToggleButtonGroup>
        <Chip size="sm" color="warning" label="Experimental" />
        <Chip
          size="sm"
          color="neutral"
          label={`soroban-ret ${data.soroban_ret_version}`}
        />
        {data.sdk_version != null && (
          <Chip size="sm" color="neutral" label={`SDK ${data.sdk_version}`} />
        )}
        {watFallback && (
          // Rust was requested but emission failed — the response degraded
          // to WAT in-place (no second round-trip). rust_error carries the
          // decompiler's reason for the curious.
          <Chip
            size="sm"
            color="warning"
            label="WAT only"
            title={data.rust_error ?? undefined}
          />
        )}
        <Box sx={{ flexGrow: 1 }} />
        <Button
          size="small"
          color="inherit"
          startIcon={<BugReportOutlinedIcon />}
          href={reportIssueUrl(
            contractId,
            data.wasm_hash,
            data.soroban_ret_version,
            data.representation
          )}
          target="_blank"
          rel="noopener noreferrer"
        >
          Report issue
        </Button>
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
      </Stack>

      <Alert severity="warning" icon={<InfoOutlinedIcon fontSize="small" />}>
        Automatically reconstructed from the on-chain WASM — not verified
        source. Signatures and types come from the contract&apos;s own metadata;
        function bodies are inferred and may be incomplete. Unrecovered values
        appear as <code>todo!()</code>.
      </Alert>

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
