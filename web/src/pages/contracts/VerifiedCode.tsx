import FileDownloadOutlinedIcon from '@mui/icons-material/FileDownloadOutlined';
import InfoOutlinedIcon from '@mui/icons-material/InfoOutlined';
import {
  Alert,
  Box,
  Button,
  Collapse,
  Link,
  Stack,
  Tooltip,
  Typography,
} from '@mui/material';
import {
  CardSkeleton,
  Chip,
  EmptyState,
  QueryErrorState,
  RelativeTimestamp,
} from '@rumblefish/soroban-block-explorer-ui';
import { useEffect, useState, type ReactNode } from 'react';

import { useContractVerifiedSource } from '../../api/index.js';
import { DisclosureRow } from '../detail/DisclosureRow.js';

import { SourceBrowser } from './SourceBrowser.js';

import type {
  BuildRecipe,
  SourceVerificationResponse,
  VerifiedSourceFile,
} from '@rumblefish/api-types';

const BLOCKSMITH_URL = 'https://staging.sv.blocksmith.co/api/query/v1/docs';

/** A verification the tab exists for: always about a hash. */
export type ShownVerification = SourceVerificationResponse & {
  wasm_hash: string;
};

/** The two states the tab exists for; everything else shows nothing. */
export function isShownVerification(
  v: SourceVerificationResponse | undefined
): v is ShownVerification {
  return (
    (v?.status === 'verified' || v?.status === 'verified_private') &&
    v.wasm_hash != null
  );
}

/** Third-party URLs become links only over http(s) — never `javascript:`. */
const safeHref = (url: string | null | undefined): string | undefined =>
  url != null && /^https?:\/\//i.test(url) ? url : undefined;

/** Every verifier that answered — for the attribution line. */
const verifierNames = (v: SourceVerificationResponse): string[] => [
  ...new Set(v.verifiers.map((x) => x.name ?? x.url)),
];

/** Only the verifiers whose own answer is `verified` — for "Verified by". */
export const verifiedByNames = (v: SourceVerificationResponse): string[] => [
  ...new Set(
    v.verifiers
      .filter((x) => x.status === 'verified')
      .map((x) => x.name ?? x.url)
  ),
];

const shortHash = (h: string) => `${h.slice(0, 8)}…${h.slice(-6)}`;

/** Same line, typography and colour as "WASM decompilation provided by Inferara". */
function Attribution({
  verification,
}: {
  verification: SourceVerificationResponse;
}) {
  const names = verifierNames(verification);
  if (names.length === 0) return null;
  return (
    <Typography
      variant="bodyXsRegular"
      sx={(theme) => ({ color: theme.palette.text.tertiary })}
    >
      Source verification by {names.join(', ')} · aggregated by{' '}
      <Link
        href={BLOCKSMITH_URL}
        target="_blank"
        rel="noopener noreferrer"
        color="inherit"
      >
        BlockSmith
      </Link>
    </Typography>
  );
}

function Toolbar({
  verification,
}: {
  verification: SourceVerificationResponse;
}) {
  const { build, source } = verification;
  const download = safeHref(source?.url);
  return (
    <Stack
      direction="row"
      spacing={1}
      alignItems="center"
      sx={{ flexWrap: 'wrap', rowGap: 1 }}
    >
      <Chip size="sm" color="emerald" label="Verified" />
      {verifiedByNames(verification).map((n) => (
        <Chip key={n} size="sm" color="neutral" label={n} />
      ))}
      {build?.package != null && (
        <Tooltip
          title={
            build.manifest_path ? `Cargo manifest: ${build.manifest_path}` : ''
          }
          arrow
        >
          <Chip size="sm" color="neutral" label={`package ${build.package}`} />
        </Tooltip>
      )}
      <Box sx={{ flexGrow: 1 }} />
      {download != null && source != null && (
        <Button
          size="small"
          color="inherit"
          startIcon={<FileDownloadOutlinedIcon />}
          href={download}
          rel="noopener noreferrer"
        >
          Download source .{source.format}
        </Button>
      )}
    </Stack>
  );
}

/** One label · value row of the recipe. */
function RecipeRow({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <Stack
      direction={{ xs: 'column', sm: 'row' }}
      spacing={{ xs: 0.5, sm: 1 }}
      alignItems={{ xs: 'flex-start', sm: 'baseline' }}
    >
      <Typography
        variant="bodyXsMedium"
        sx={(theme) => ({
          color: theme.palette.text.secondary,
          minWidth: { sm: 150 },
          flexShrink: 0,
        })}
      >
        {label}
      </Typography>
      <Stack spacing={0.5} alignItems="flex-start" sx={{ minWidth: 0 }}>
        {children}
      </Stack>
    </Stack>
  );
}

/**
 * A value exactly as the verifier published it: every space and newline
 * kept, its text direction isolated, inside a frame so nothing in it can
 * pass for a label. An empty string still shows as an empty frame.
 */
function Value({ text }: { text: string }) {
  return (
    <Typography
      variant="bodyXsRegular"
      component="span"
      sx={(theme) => ({
        display: 'block',
        minHeight: '1.5em',
        minWidth: '2ch',
        px: 0.75,
        borderRadius: `${theme.shape.radius.s}px`,
        borderLeft: `2px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.grayMainAlt,
        color: theme.palette.text.secondary,
        fontFamily: 'monospace',
        whiteSpace: 'pre-wrap',
        // Visual break points only; the copied text is unchanged.
        overflowWrap: 'anywhere',
        unicodeBidi: 'isolate',
      })}
    >
      {text}
    </Typography>
  );
}

/** Our own words for a missing value — never in a frame, so they cannot be
 *  mistaken for something the verifier published. */
function Missing({ children }: { children: ReactNode }) {
  return (
    <Typography
      variant="bodyXsRegular"
      component="span"
      sx={(theme) => ({
        color: theme.palette.text.tertiary,
        fontStyle: 'italic',
      })}
    >
      {children}
    </Typography>
  );
}

/** A published list: one frame per element, in order. */
function ValueList({ items, empty }: { items: string[]; empty: string }) {
  if (items.length === 0) return <Missing>{empty}</Missing>;
  return (
    <>
      {items.map((item, i) => (
        <Value key={i} text={item} />
      ))}
    </>
  );
}

/** The recipe to reproduce the build, in the Decompilation tab's
 *  "Binary features" box, collapsed behind the same chevron as the tree. */
function BuildRecipeBox({
  build,
  sha256,
}: {
  build: BuildRecipe;
  /** The archive's sha256 — absent when the source is not published. */
  sha256?: string;
}) {
  const [open, setOpen] = useState(false);
  const report = safeHref(build.report_url);
  return (
    <Stack
      spacing={1}
      sx={(theme) => ({
        p: 1.5,
        borderRadius: `${theme.shape.radius.s}px`,
        border: `1px solid ${theme.palette.stroke.default}`,
      })}
    >
      <DisclosureRow
        open={open}
        onToggle={() => setOpen((o) => !o)}
        label="Build recipe"
      />
      <Collapse in={open} unmountOnExit>
        <Stack spacing={1.25}>
          <RecipeRow label="Verified at">
            {build.verified_at != null ? (
              <Stack direction="row" spacing={1} alignItems="baseline">
                <Value text={build.verified_at} />
                <Missing>
                  <RelativeTimestamp
                    timestamp={build.verified_at}
                    variant="bodyXsRegular"
                  />
                </Missing>
              </Stack>
            ) : (
              <Missing>not published</Missing>
            )}
          </RecipeRow>
          <RecipeRow label="Package">
            {build.package != null ? (
              <Value text={build.package} />
            ) : (
              <Missing>not given</Missing>
            )}
          </RecipeRow>
          <RecipeRow label="Cargo manifest">
            {build.manifest_path != null ? (
              <Value text={build.manifest_path} />
            ) : (
              <Missing>not given — the workspace root</Missing>
            )}
          </RecipeRow>
          <RecipeRow label="Build image">
            {build.image != null ? (
              <Value text={build.image} />
            ) : (
              <Missing>not published</Missing>
            )}
          </RecipeRow>
          <RecipeRow label="Build options">
            <ValueList items={build.options} empty="none" />
          </RecipeRow>
          <RecipeRow label="Build arguments">
            <ValueList
              items={build.arguments}
              empty="empty list — the default, contract build"
            />
          </RecipeRow>
          {sha256 != null && (
            <RecipeRow label="Source archive sha256">
              <Value text={sha256} />
            </RecipeRow>
          )}
          {report != null && (
            <Typography variant="bodyXsRegular">
              <Link
                href={report}
                target="_blank"
                rel="noopener noreferrer"
                color="inherit"
              >
                Full verification report (JSON)
              </Link>
            </Typography>
          )}
        </Stack>
      </Collapse>
    </Stack>
  );
}

function StatsLine({ files }: { files: VerifiedSourceFile[] }) {
  const bytes = files.reduce((s, f) => s + f.size, 0);
  const size =
    bytes >= 1024 * 1024
      ? `${(bytes / 1024 / 1024).toFixed(1)} MB`
      : `${Math.round(bytes / 1024)} KB`;
  return (
    <Typography
      variant="bodyXsRegular"
      sx={(theme) => ({ color: theme.palette.text.tertiary })}
    >
      {files.length} files · {size} unpacked · matches the published sha256
    </Typography>
  );
}

function PublishedSource({
  contractId,
  verification,
}: {
  contractId: string;
  verification: ShownVerification;
}) {
  const { data, isPending, isFetching, isError, error, refetch } =
    useContractVerifiedSource(contractId);
  // Cached files are kept for the session; if the contract has since moved to
  // other verified code, they belong to the old hash and must be fetched anew.
  // One retry: the browser may keep serving the old files for the cache's
  // five minutes, and a loop of skeletons would hide that.
  const stale = data != null && data.wasm_hash !== verification.wasm_hash;
  const [retried, setRetried] = useState(false);
  useEffect(() => {
    if (stale && !isFetching && !retried) {
      setRetried(true);
      void refetch();
    }
  }, [stale, isFetching, retried, refetch]);
  if (isPending || (stale && (!retried || isFetching))) return <CardSkeleton />;
  if (stale) {
    return (
      <EmptyState
        icon={<InfoOutlinedIcon fontSize="small" />}
        title="The contract's code changed while this page was open"
        description="Reload in a few minutes to see the source of the new code."
      />
    );
  }
  if (isError || data == null) {
    return <QueryErrorState error={error} onRetry={() => void refetch()} />;
  }
  return (
    <>
      <StatsLine files={data.files} />
      {verification.build != null && (
        <BuildRecipeBox
          build={verification.build}
          sha256={data.archive_sha256}
        />
      )}
      <SourceBrowser
        files={data.files}
        manifestPath={verification.build?.manifest_path}
      />
    </>
  );
}

/**
 * Verified Code tab (task 0639): source code an independent verifier
 * rebuilt to the exact on-chain WASM, read through the BlockSmith Query
 * API. Built from the Decompilation tab's parts (`ContractCode.tsx`): chip
 * toolbar with Download on the right, one Alert, a stats line, a bordered
 * details box, `CodeHighlight` with `CopyButton`, the attribution line.
 *
 * Mounted only for `verified` and `verified_private` — results that did not
 * reproduce the on-chain code show nothing anywhere.
 */
export function VerifiedCode({
  contractId,
  verification,
}: {
  contractId: string;
  verification: ShownVerification;
}) {
  const names = verifiedByNames(verification).join(', ');
  return (
    <Stack spacing={1.5} sx={{ p: 2 }}>
      <Toolbar verification={verification} />
      {verification.status === 'verified' ? (
        <>
          <Alert
            severity="success"
            icon={<InfoOutlinedIcon fontSize="small" />}
          >
            Verified source — rebuilding this source reproduces the on-chain
            WASM <code>{shortHash(verification.wasm_hash)}</code> byte for byte.
          </Alert>
          <PublishedSource
            contractId={contractId}
            verification={verification}
          />
        </>
      ) : (
        <>
          <Alert
            severity="success"
            icon={<InfoOutlinedIcon fontSize="small" />}
          >
            Verified by {names} — the source code is not published.
          </Alert>
          {verification.build != null && (
            <BuildRecipeBox build={verification.build} />
          )}
        </>
      )}
      <Attribution verification={verification} />
    </Stack>
  );
}
