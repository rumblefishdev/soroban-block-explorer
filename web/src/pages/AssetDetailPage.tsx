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
import { assetTypeMeta } from './assets/assetType.js';
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
              size={40}
            />
          )}
          <Typography variant="heading3SemiBold" component="h1">
            {code}
          </Typography>
          {meta && <Chip size="md" color={meta.color} label={meta.label} />}
        </Stack>
        {data?.name && (
          <Typography variant="bodyRegular" sx={{ color: 'text.secondary' }}>
            {data.name}
          </Typography>
        )}
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

      {/* The embedded transactions list fetches by asset id; rendering it
          while the parent asset query is loading or errored would either
          show a duplicate skeleton (loading) or a duplicate error banner
          (4xx/5xx) on top of the unified `NotFoundState` / `GenericErrorState`
          already rendered in the summary box above. Gate the section
          strictly on `data` so it appears only when the parent succeeded. */}
      {asset.data && (
        <SectionErrorBoundary sectionName="asset-transactions">
          <AssetTransactions assetId={id} />
        </SectionErrorBoundary>
      )}
    </Stack>
  );
}
