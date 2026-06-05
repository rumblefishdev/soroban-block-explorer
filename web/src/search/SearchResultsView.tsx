import { Box, Stack, Typography } from '@mui/material';

import {
  QueryErrorState,
  SearchSpinner,
} from '@rumblefish/soroban-block-explorer-ui';

import { SearchResultRow } from './SearchResultRow.js';
import { SearchResultsTabs } from './SearchResultsTabs.js';
import { type SearchResultsState } from './useSearchResults.js';

interface SearchResultsViewProps {
  state: SearchResultsState;

  highlightedIndex?: number;
  onRowMouseEnter?: (index: number) => void;
  onRowClick?: (index: number) => void;
  maxListHeight?: number | string;
  emptyCopy?: { title: string; description?: string };
}

export function SearchResultsView({
  state,
  highlightedIndex = -1,
  onRowMouseEnter,
  onRowClick,
  maxListHeight,
  emptyCopy,
}: SearchResultsViewProps) {
  const {
    effectiveQuery,
    data,
    isFetching,
    isError,
    error,
    refetch,
    counts,
    totalCount,
    activeTab,
    setActiveTab,
    hitsForActiveTab,
  } = state;

  const showResults = data != null && totalCount > 0;

  return (
    <Stack>
      <SearchResultsTabs
        activeTab={activeTab}
        onChange={setActiveTab}
        counts={counts}
      />
      <Box
        sx={(theme) =>
          maxListHeight != null
            ? {
                maxHeight: maxListHeight,
                overflowY: 'auto',
                scrollbarWidth: 'thin',
                scrollbarColor: `${theme.palette.surface.grayLight} transparent`,
                '&::-webkit-scrollbar': { width: 6 },
                '&::-webkit-scrollbar-track': { background: 'transparent' },
                '&::-webkit-scrollbar-thumb': {
                  background: theme.palette.surface.grayLight,
                  borderRadius: 3,
                },
                '&::-webkit-scrollbar-thumb:hover': {
                  background: theme.palette.stroke.defaultHover,
                },
              }
            : {}
        }
      >
        {isFetching && !showResults && <SearchSpinner />}
        {isError && <QueryErrorState error={error} onRetry={refetch} py={4} />}
        {!isFetching &&
          !isError &&
          effectiveQuery.length > 0 &&
          totalCount === 0 && (
            <Stack spacing={0.5} alignItems="center" sx={{ p: 3 }}>
              <Typography
                variant="bodySmMedium"
                sx={(theme) => ({ color: theme.palette.text.secondary })}
              >
                {emptyCopy?.title ?? `No results for "${effectiveQuery}"`}
              </Typography>
              <Typography
                variant="bodyXsRegular"
                sx={(theme) => ({
                  color: theme.palette.text.tertiary,
                  textAlign: 'center',
                })}
              >
                {emptyCopy?.description ??
                  'Try a full transaction hash, account address (G…), contract address (C…), liquidity pool (L…), or token code.'}
              </Typography>
            </Stack>
          )}
        {!isFetching && !isError && effectiveQuery.length === 0 && (
          <Stack alignItems="center" sx={{ p: 3 }}>
            <Typography
              variant="bodyXsRegular"
              sx={(theme) => ({
                color: theme.palette.text.tertiary,
                textAlign: 'center',
              })}
            >
              Type to search transactions, accounts, contracts, tokens, NFTs,
              and liquidity pools.
            </Typography>
          </Stack>
        )}
        {showResults && hitsForActiveTab.length === 0 && (
          <Stack alignItems="center" sx={{ p: 3 }}>
            <Typography
              variant="bodyXsRegular"
              sx={(theme) => ({ color: theme.palette.text.tertiary })}
            >
              No results in this category.
            </Typography>
          </Stack>
        )}
        {showResults &&
          hitsForActiveTab.map((hit, idx) => (
            <SearchResultRow
              key={`${hit.entity_type}:${
                hit.surrogate_id ?? hit.identifier
              }:${idx}`}
              hit={hit}
              highlighted={idx === highlightedIndex}
              onMouseEnter={() => onRowMouseEnter?.(idx)}
              onClick={() => onRowClick?.(idx)}
            />
          ))}
      </Box>
    </Stack>
  );
}
