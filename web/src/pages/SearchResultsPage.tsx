import { Box, Stack, Typography } from '@mui/material';
import { useEffect } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';

import {
  routeForHit,
  SearchInput,
} from '@rumblefish/soroban-block-explorer-ui';

import { directRouteFor } from '../search/directRouteFor.js';
import { SearchResultsView } from '../search/SearchResultsView.js';
import { useSearchResults } from '../search/useSearchResults.js';

export default function SearchResultsPage() {
  const [params, setParams] = useSearchParams();
  const navigate = useNavigate();
  const q = params.get('q') ?? '';
  const state = useSearchResults({ q });

  // Deep-link / paste path (`/search?q=...`) bypasses the AppShell +
  // HomeHero submit handlers that normally call `directRouteFor`
  // first. Re-run the FE classifier here so a ledger sequence pasted
  // into the URL bar (or typed into this page's own `SearchInput`,
  // which writes back to the `q` param via `setParams`) lands on
  // `/ledgers/<seq>` instead of an empty broad-search results page.
  useEffect(() => {
    const target = directRouteFor(q);
    if (target) {
      navigate(target, { replace: true });
    }
  }, [q, navigate]);

  // Singleton auto-navigation (task 0271): when broad search returns
  // exactly one hit across all entity buckets, treat it as a redirect
  // and navigate straight to the detail page. `routeForHit` already
  // handles every entity type (asset uses surrogate_id, NFT uses
  // composite contract_id+token_id, others use identifier).
  useEffect(() => {
    if (state.isFetching || state.totalCount !== 1) return;
    const [singleton] = state.hitsForActiveTab;
    if (!singleton) return;
    navigate(routeForHit(singleton), { replace: true });
  }, [state.isFetching, state.totalCount, state.hitsForActiveTab, navigate]);

  return (
    <Stack spacing={3} sx={{ py: 2 }}>
      <Box>
        {/* Heading needs an explicit `component="h1"` — without it both
            Typography blocks default to plain divs, so the a11y tree
            concatenates them into one run ("SearchRefine your query…").
            Aligning with the other list/detail pages, which all set the
            heading element explicitly (task 0251 H10). */}
        <Typography variant="heading4Bold" component="h1" sx={{ mb: 0.5 }}>
          Search
        </Typography>
        <Typography
          variant="bodySmRegular"
          component="p"
          sx={{ color: 'text.tertiary' }}
        >
          Refine your query to find transactions, accounts, contracts, tokens,
          NFTs, and liquidity pools.
        </Typography>
      </Box>

      <Box sx={{ maxWidth: 628 }}>
        <SearchInput
          size="lg"
          value={q}
          onChange={(next) => {
            const nextParams = new URLSearchParams(params);
            if (next.length > 0) nextParams.set('q', next);
            else nextParams.delete('q');
            setParams(nextParams, { replace: true });
          }}
          onClear={() => {
            const nextParams = new URLSearchParams(params);
            nextParams.delete('q');
            setParams(nextParams, { replace: true });
          }}
        />
      </Box>

      <SearchResultsView state={state} />
    </Stack>
  );
}
