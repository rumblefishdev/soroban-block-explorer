import { Box, Link, Stack, Typography } from '@mui/material';
import {
  classifyError,
  DetailSkeleton,
  GenericErrorState,
  isMissingResource,
  NotFoundState,
  RateLimitState,
  SectionErrorBoundary,
  TransientErrorState,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
import { Link as RouterLink, useParams } from 'react-router-dom';

import { useNftDetail } from '../api/index.js';
import { routes } from '../router/routes.js';

import { NftMediaPreview } from './nft-detail/NftMediaPreview.js';
import { NftMetadata } from './nft-detail/NftMetadata.js';
import { NftSummary } from './nft-detail/NftSummary.js';
import { NftTransfers } from './nft-detail/NftTransfers.js';

function Breadcrumb({
  collection,
  tokenId,
}: {
  collection?: string | null;
  tokenId: string;
}) {
  const sep = (
    <Typography variant="bodySmRegular" sx={{ color: 'text.tertiary' }}>
      /
    </Typography>
  );
  return (
    <Box sx={{ display: 'flex', gap: 0.5, flexWrap: 'wrap', mb: 1 }}>
      <Link
        component={RouterLink}
        to={routes.nfts}
        variant="bodySmRegular"
        underline="hover"
        sx={{ color: 'text.tertiary' }}
      >
        NFTs
      </Link>
      {collection ? (
        <>
          {sep}
          <Typography variant="bodySmRegular" sx={{ color: 'text.tertiary' }}>
            {collection}
          </Typography>
        </>
      ) : null}
      {sep}
      <Typography variant="bodySmRegular" sx={{ color: 'text.primary' }}>
        #{tokenId}
      </Typography>
    </Box>
  );
}

export default function NftDetailPage() {
  const { id: rawId } = useParams<{ id: string }>();
  // The detail route is keyed by the numeric `nfts.id` surrogate.
  const valid = rawId != null && /^\d+$/.test(rawId) && Number(rawId) > 0;
  const id = valid ? Number(rawId) : Number.NaN;

  const {
    data: nft,
    isLoading,
    isError,
    error,
    refetch,
  } = useNftDetail(id, valid);

  if (!valid) {
    return <NotFoundState titleOverride="NFT not found" identifier={rawId} />;
  }

  if (isLoading) {
    return <DetailSkeleton />;
  }

  if (isError) {
    const kind = classifyError(error);
    if (isMissingResource(kind)) {
      // 400 (e.g. non-numeric or i64-overflow id) and 404 both mean
      // "this NFT isn't here" — single NotFound (task 0251 H8).
      return <NotFoundState titleOverride="NFT not found" identifier={rawId} />;
    }
    const retry = () => void refetch();
    return (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 8 }}>
        {kind === 'rate-limit' ? (
          <RateLimitState onRetry={retry} />
        ) : kind === 'transient' ? (
          <TransientErrorState onRetry={retry} />
        ) : (
          <GenericErrorState onRetry={retry} />
        )}
      </Box>
    );
  }

  if (!nft) {
    return <NotFoundState titleOverride="NFT not found" identifier={rawId} />;
  }

  const title = nft.name?.trim() || `Token ${nft.token_id}`;
  const collectionLine: ReactNode = nft.collection_name ? (
    <Typography variant="bodyRegular" sx={{ color: 'text.secondary' }}>
      Collection:{' '}
      <Box component="span" sx={{ fontWeight: 700 }}>
        {nft.collection_name}
      </Box>
    </Typography>
  ) : null;

  return (
    <Stack spacing={3}>
      <Box>
        <Breadcrumb collection={nft.collection_name} tokenId={nft.token_id} />
      </Box>

      <Box
        sx={{
          display: 'flex',
          gap: 3,
          alignItems: 'flex-start',
          flexWrap: 'wrap',
        }}
      >
        <SectionErrorBoundary sectionName="NFT media">
          <NftMediaPreview mediaUrl={nft.media_url} name={title} />
        </SectionErrorBoundary>

        <Stack spacing={2} sx={{ flex: 1, minWidth: 320 }}>
          <Box>
            <Typography variant="heading5SemiBold" component="h1">
              {title}
            </Typography>
            {collectionLine}
          </Box>

          <SectionErrorBoundary sectionName="NFT details">
            <NftSummary nft={nft} />
          </SectionErrorBoundary>

          <SectionErrorBoundary sectionName="NFT traits">
            <NftMetadata metadata={nft.metadata} />
          </SectionErrorBoundary>
        </Stack>
      </Box>

      <SectionErrorBoundary sectionName="NFT transfer history">
        <NftTransfers nftId={id} />
      </SectionErrorBoundary>
    </Stack>
  );
}
