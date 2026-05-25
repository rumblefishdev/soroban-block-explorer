import { Box, Stack, Typography } from '@mui/material';
import { useEffect } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';

import { SearchInput } from '@rumblefish/soroban-block-explorer-ui';

import { routeForHit } from '../search/routeForHit.js';
import { SearchResultsView } from '../search/SearchResultsView.js';
import { useSearchResults } from '../search/useSearchResults.js';

export default function SearchResultsPage() {
  const [params, setParams] = useSearchParams();
  const navigate = useNavigate();
  const q = params.get('q') ?? '';
  const state = useSearchResults({ q });

  useEffect(() => {
    if (state.data?.type === 'redirect') {
      navigate(
        routeForHit({
          entity_type: state.data.entity_type,
          identifier: state.data.entity_id,
        }),
        { replace: true }
      );
    }
  }, [state.data, navigate]);

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
