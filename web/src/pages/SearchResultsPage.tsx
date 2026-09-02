import { Box, Paper, Stack, Typography } from '@mui/material';
import { useEffect } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';

import { SearchInput } from '@rumblefish/soroban-block-explorer-ui';

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

  // No singleton auto-navigation. Task 0271 sent a one-hit broad search
  // straight to that hit; 0527 withdrew it — it took the page away before
  // the match could be read, and `replace: true` meant Back could not bring
  // it back. Accepted cost: a pasted full tx hash or StrKey also matched
  // exactly one row and rode this effect, so it now stops on a one-row
  // results page — one click from the detail page. Only a bare ledger
  // sequence still redirects directly (`directRouteFor` above); /v1/search
  // has had no redirect branch since 0271.

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
          sx={(theme) => ({ color: theme.palette.text.tertiary })}
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

      {/* Rounded outlined card with a surface background (results + tab headers
          read as one block) — using the explorer's standard table/card border
          (`stroke.default`, 1px), not the nav-bar dropdown's accent border. */}
      <Paper
        variant="outlined"
        elevation={0}
        sx={(theme) => ({
          borderColor: theme.palette.stroke.default,
          backgroundColor: theme.palette.surface.grayMain,
          overflow: 'hidden',
        })}
      >
        <SearchResultsView state={state} />
      </Paper>
    </Stack>
  );
}
