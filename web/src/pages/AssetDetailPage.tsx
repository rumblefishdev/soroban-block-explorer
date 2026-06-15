import { Box, Stack, Typography } from '@mui/material';
import {
  Chip,
  DetailErrorState,
  isAssetId,
  NotFoundState,
  SectionErrorBoundary,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
import { useParams } from 'react-router-dom';

import { useAssetDetail } from '../api/index.js';
import { routes } from '../router/routes.js';

import { AssetDetailSkeleton } from './assets/AssetDetailSkeleton.js';
import { AssetIcon } from './assets/AssetIcon.js';
import { AssetMetadata } from './assets/AssetMetadata.js';
import { AssetSummary } from './assets/AssetSummary.js';
import { AssetTransactions } from './assets/AssetTransactions.js';
import { assetTypeMeta } from './assets/assetType.js';
import { PageBreadcrumb } from './detail/PageBreadcrumb.js';

/**
 * Asset detail page (`/assets/:id`) — summary, TOML metadata, and a paginated
 * transaction history. Covers native XLM, classic credit assets, SACs and
 * Soroban tokens; the type badge keeps their differences explicit.
 */
export default function AssetDetailPage() {
  const { id = '' } = useParams<{ id: string }>();
  // Pre-validate the param like the sibling detail pages (skip the fetch +
  // render NotFound on a malformed id). Post-0243 the asset id is the
  // canonical token (`native` | contract StrKey | `CODE-ISSUER`) — surrogate
  // routing is gone — so `isAssetId` is the correct guard.
  const valid = isAssetId(id);
  const asset = useAssetDetail(valid ? id : '');

  if (!valid) {
    return <NotFoundState entity="asset" identifier={id} />;
  }

  if (asset.isLoading) {
    return <AssetDetailSkeleton />;
  }

  const data = asset.data;
  const code = data?.asset_code ?? 'Asset';
  const meta = data ? assetTypeMeta(data.asset_type_name) : null;

  let summary: ReactNode = null;
  let metadata: ReactNode = null;
  if (asset.isError) {
    summary = (
      <DetailErrorState
        error={asset.error}
        entity="asset"
        identifier={id}
        onRetry={() => void asset.refetch()}
        py={6}
      />
    );
  } else if (data) {
    summary = <AssetSummary asset={data} />;
    metadata = <AssetMetadata asset={data} />;
  }

  return (
    <Stack spacing={3}>
      <Box>
        <PageBreadcrumb
          items={[{ label: 'Assets', to: routes.assets }, { label: code }]}
        />
        <Stack direction="row" spacing={1.5} alignItems="center">
          {data && (
            <AssetIcon
              code={data.asset_code}
              iconUrl={data.icon_url}
              size={40}
            />
          )}
          <Box sx={{ minWidth: 0 }}>
            <Stack direction="row" spacing={1} alignItems="center">
              <Typography variant="heading5SemiBold" component="h1">
                {code}
              </Typography>
              {meta && <Chip size="sm" color={meta.color} label={meta.label} />}
            </Stack>
            {data?.name && (
              <Typography
                variant="bodyMedium"
                sx={(theme) => ({ color: theme.palette.text.secondary })}
              >
                {data.name}
              </Typography>
            )}
          </Box>
        </Stack>
      </Box>

      {asset.isError ? (
        <SectionErrorBoundary sectionName="asset-summary">
          {summary}
        </SectionErrorBoundary>
      ) : (
        <Box
          sx={{
            display: 'flex',
            flexDirection: { xs: 'column', md: 'row' },
            gap: 3,
            alignItems: 'flex-start',
          }}
        >
          <Box sx={{ flex: 1, minWidth: 0, width: '100%' }}>
            <SectionErrorBoundary sectionName="asset-summary">
              {summary}
            </SectionErrorBoundary>
          </Box>
          <Box sx={{ flex: 1, minWidth: 0, width: '100%' }}>
            <SectionErrorBoundary sectionName="asset-metadata">
              {metadata}
            </SectionErrorBoundary>
          </Box>
        </Box>
      )}

      {/* `AssetTransactions` is an independent query with its own
          `TableSkeleton` / error handling, so we keep it mounted while
          the parent asset query is still loading — that way the page
          shows a consistent skeleton row (summary card + metadata card
          + transactions table skeleton) instead of the transactions
          section popping in only after the parent settles. Only hide
          on parent error: a 400 / 404 / 5xx on the asset itself means
          the asset doesn't exist (or the API is degraded), in which
          case the embedded list would just surface a duplicate banner
          below the already-routed DetailErrorState. */}
      {!asset.isError && (
        <SectionErrorBoundary sectionName="asset-transactions">
          <AssetTransactions assetId={id} />
        </SectionErrorBoundary>
      )}
    </Stack>
  );
}
