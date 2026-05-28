import { Box, Stack, Typography } from '@mui/material';
import {
  CardSkeleton,
  Chip,
  classifyError,
  GenericErrorState,
  isMissingResource,
  NotFoundState,
  SectionErrorBoundary,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
import { useParams } from 'react-router-dom';

import { useAssetDetail } from '../api/index.js';
import { routes } from '../router/routes.js';

import { AssetIcon } from './assets/AssetIcon.js';
import { AssetMetadata } from './assets/AssetMetadata.js';
import { AssetSummary } from './assets/AssetSummary.js';
import { AssetTransactions } from './assets/AssetTransactions.js';
import { assetTypeMeta, iconKindFor } from './assets/assetType.js';
import { PageBreadcrumb } from './detail/PageBreadcrumb.js';

/**
 * Asset detail page (`/assets/:id`) — summary, TOML metadata, and a paginated
 * transaction history. Covers native XLM, classic credit assets, SACs and
 * Soroban tokens; the type badge keeps their differences explicit.
 */
export default function AssetDetailPage() {
  const { id = '' } = useParams<{ id: string }>();
  const asset = useAssetDetail(id);

  if (id === '') {
    return (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 8 }}>
        <NotFoundState entity="asset" />
      </Box>
    );
  }

  const data = asset.data;
  const code = data?.asset_code ?? 'Asset';
  const meta = data ? assetTypeMeta(data.asset_type_name) : null;

  let summary: ReactNode = null;
  let metadata: ReactNode = null;
  if (asset.isLoading) {
    summary = <CardSkeleton />;
    metadata = <CardSkeleton />;
  } else if (asset.isError) {
    summary = (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 6 }}>
        {isMissingResource(classifyError(asset.error)) ? (
          <NotFoundState entity="asset" identifier={id} />
        ) : (
          <GenericErrorState onRetry={() => void asset.refetch()} />
        )}
      </Box>
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
              kind={iconKindFor(data.asset_type_name)}
              size={40}
            />
          )}
          <Box sx={{ minWidth: 0 }}>
            <Stack direction="row" spacing={1} alignItems="center">
              <Typography variant="heading4SemiBold" component="h1">
                {code}
              </Typography>
              {meta && <Chip size="sm" color={meta.color} label={meta.label} />}
            </Stack>
            {data?.name && (
              <Typography
                variant="bodyRegular"
                sx={{ color: 'text.secondary' }}
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
          below the already-routed NotFoundState / GenericErrorState. */}
      {!asset.isError && (
        <SectionErrorBoundary sectionName="asset-transactions">
          <AssetTransactions assetId={id} />
        </SectionErrorBoundary>
      )}
    </Stack>
  );
}
