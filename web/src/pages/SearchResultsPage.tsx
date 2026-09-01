import { Box, CircularProgress, Paper, Stack, Typography } from '@mui/material';
import { useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';

import { SearchInput } from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../router/routes.js';
import { directRouteFor } from '../search/directRouteFor.js';
import { federatedDomain, resolveFederated } from '../search/federation.js';
import { SearchResultsView } from '../search/SearchResultsView.js';
import { useSearchResults } from '../search/useSearchResults.js';

export default function SearchResultsPage() {
  const [params, setParams] = useSearchParams();
  const navigate = useNavigate();
  const q = params.get('q') ?? '';

  // The input is driven by local state, not by the URL. Controlling it
  // directly off `params` put the value one render behind the keystroke, so
  // React re-assigned `input.value` afterwards and the browser dropped the
  // caret at the end — editing mid-query was impossible. The URL is still
  // written on every change and is still the shareable value; it just is no
  // longer what the field reads back (task 0527 #1).
  const [text, setText] = useState(q);
  useEffect(() => {
    setText(q);
  }, [q]);

  function writeQuery(next: string) {
    setText(next);
    const nextParams = new URLSearchParams(params);
    if (next.length > 0) nextParams.set('q', next);
    else nextParams.delete('q');
    setParams(nextParams, { replace: true });
  }
  const federatedFor = federatedDomain(q);
  // A federated input is not a broad-search query — skip `/v1/search`
  // entirely rather than paying for a request whose answer is always empty.
  const state = useSearchResults({ q: federatedFor == null ? q : '' });

  // SEP-2 federated address (`name*domain`) typed into search — task 0443
  // scope A. `directRouteFor` cannot carry this: it is synchronous, and the
  // resolve is two network round-trips. It hooks in here instead, the one
  // point the app-shell bar, the home hero and a pasted `/search?q=` URL all
  // converge on, so neither caller needs to change.
  const federated = useQuery({
    queryKey: ['federation', q],
    queryFn: () => resolveFederated(q),
    enabled: federatedFor != null,
    retry: false,
    staleTime: 5 * 60_000,
  });

  useEffect(() => {
    if (federated.data?.kind !== 'resolved') return;
    navigate(routes.account(federated.data.accountId), { replace: true });
  }, [federated.data, navigate]);

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
  // it back. Deterministic redirects are untouched: a tx hash, a StrKey and
  // a ledger sequence are exact-identity lookups resolved before the search
  // runs, so they still land on their page directly.

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
          value={text}
          onChange={writeQuery}
          onClear={() => writeQuery('')}
        />
      </Box>

      {/* Rounded outlined card with a surface background (results + tab headers
          read as one block) — using the explorer's standard table/card border
          (`stroke.default`, 1px), not the nav-bar dropdown's accent border. */}
      {/* A federated input has no business reaching the broad-search buckets:
          `/v1/search` knows nothing about SEP-2 and would return zero hits,
          which renders as "no results" — a claim that the address does not
          exist. Show the resolve's own state instead, and only that. */}
      {federatedFor != null ? (
        <Paper
          variant="outlined"
          elevation={0}
          sx={(theme) => ({
            borderColor:
              federated.data?.kind === 'failed'
                ? theme.palette.stroke.error
                : theme.palette.stroke.default,
            backgroundColor:
              federated.data?.kind === 'failed'
                ? theme.palette.surface.error
                : theme.palette.surface.grayMain,
            px: 2,
            py: 1.5,
          })}
        >
          <Stack direction="row" spacing={1.5} alignItems="center">
            {federated.data?.kind !== 'failed' && (
              <CircularProgress size={16} thickness={5} />
            )}
            <Typography
              variant="bodySmRegular"
              component="p"
              role="status"
              sx={(theme) => ({
                color:
                  federated.data?.kind === 'failed'
                    ? theme.palette.text.error
                    : theme.palette.text.tertiary,
              })}
            >
              {federated.data?.kind === 'failed'
                ? federated.data.reason
                : `Resolving ${q} with ${federatedFor}…`}
            </Typography>
          </Stack>
        </Paper>
      ) : (
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
      )}
    </Stack>
  );
}
