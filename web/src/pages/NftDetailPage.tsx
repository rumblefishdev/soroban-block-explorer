import { Box, Link, Stack, Typography } from '@mui/material';
import {
  DetailErrorState,
  DetailSkeleton,
  isContractId,
  NotFoundState,
  SectionErrorBoundary,
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
    <Typography
      variant="bodySmMedium"
      sx={(theme) => ({ color: theme.palette.text.tertiary })}
    >
      /
    </Typography>
  );
  return (
    <Box sx={{ display: 'flex', gap: 0.5, flexWrap: 'wrap', mb: 1 }}>
      <Link
        component={RouterLink}
        to={routes.nfts}
        variant="bodySmMedium"
        underline="hover"
        sx={(theme) => ({ color: theme.palette.text.tertiary })}
      >
        NFTs
      </Link>
      {collection ? (
        <>
          {sep}
          <Typography
            variant="bodySmMedium"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            {collection}
          </Typography>
        </>
      ) : null}
      {sep}
      <Typography
        variant="bodySmMedium"
        sx={(theme) => ({ color: theme.palette.text.primary })}
      >
        #{tokenId}
      </Typography>
    </Box>
  );
}

export default function NftDetailPage() {
  // react-router-dom v6+ already URL-decodes path params, so the value
  // here is the canonical token_id; manual decodeURIComponent would
  // double-decode any `%` literal in the token (e.g. `foo%bar`).
  const { contractId = '', tokenId = '' } = useParams<{
    contractId: string;
    tokenId: string;
  }>();
  // contract_id must be a valid C-strkey; token_id is opaque (≤128 ASCII).
  const valid =
    isContractId(contractId) && tokenId !== '' && tokenId.length <= 128;
  const identifier = `${contractId}/${tokenId}`;

  const {
    data: nft,
    isLoading,
    isError,
    error,
    refetch,
  } = useNftDetail(contractId, tokenId, valid);

  if (!valid) {
    return (
      <NotFoundState titleOverride="NFT not found" identifier={identifier} />
    );
  }

  if (isLoading) {
    return <DetailSkeleton />;
  }

  if (isError) {
    // 400 (e.g. malformed strkey / oversized token_id) and 404 both mean
    // "this NFT isn't here" — single NotFound (task 0251 H8); other errors
    // get the shared rate-limit / transient / generic handling.
    return (
      <DetailErrorState
        error={error}
        entity="generic"
        titleOverride="NFT not found"
        identifier={identifier}
        onRetry={() => void refetch()}
        py={8}
      />
    );
  }

  if (!nft) {
    return (
      <NotFoundState titleOverride="NFT not found" identifier={identifier} />
    );
  }

  const title = nft.name?.trim() || `Token ${nft.token_id}`;
  const collectionLine: ReactNode = nft.collection_name ? (
    <Typography
      variant="bodyMedium"
      sx={(theme) => ({ color: theme.palette.text.secondary })}
    >
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
        <NftTransfers contractId={contractId} tokenId={tokenId} />
      </SectionErrorBoundary>
    </Stack>
  );
}
