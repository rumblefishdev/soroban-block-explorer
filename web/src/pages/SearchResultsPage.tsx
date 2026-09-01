import { Box, CircularProgress, Paper, Stack, Typography } from '@mui/material';
import { useEffect, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';

import { SearchInput } from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../router/routes.js';
import { directRouteFor } from '../search/directRouteFor.js';
import { SearchResultsView } from '../search/SearchResultsView.js';
import { useFederatedAddress } from '../search/useFederation.js';
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
  // longer what the field reads back. The effect exists only for history
  // navigation and remount (task 0527 #1).
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

  // Escape hatch for the "search this as text" action below; a new query
  // starts classified again.
  const [asText, setAsText] = useState(false);
  useEffect(() => {
    setAsText(false);
  }, [q]);

  // The search hook classifies the query too and suppresses its own request
  // for a federated address, so the two never disagree about what the buckets
  // should be asked.
  const state = useSearchResults({ q, asText });

  // SEP-2 federated address (`name*domain`) typed into search — task 0443.
  // `directRouteFor` cannot carry this: it is synchronous, and the resolve is
  // two network round-trips. It hooks in here instead, the one point the
  // app-shell bar, the home hero and a pasted `/search?q=` URL all converge
  // on, so neither caller needs to change.
  const federated = useFederatedAddress(asText ? '' : q);
  const federatedFor = federated.domain;
  const failure =
    federated.data?.kind === 'failed' ? federated.data.reason : null;

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
          (`stroke.default`, 1px), not the nav-bar dropdown's accent border.
          A federated input takes the same card over: `/v1/search` knows
          nothing about SEP-2, so its empty answer would render as "no
          results" — a claim that the address does not exist. */}
      <Paper
        variant="outlined"
        elevation={0}
        sx={(theme) => ({
          borderColor:
            failure != null
              ? theme.palette.stroke.error
              : theme.palette.stroke.default,
          backgroundColor:
            failure != null
              ? theme.palette.surface.error
              : theme.palette.surface.grayMain,
          overflow: 'hidden',
        })}
      >
        {federatedFor == null ? (
          <SearchResultsView state={state} />
        ) : (
          <Stack spacing={1} sx={{ px: 2, py: 1.5 }}>
            <Stack direction="row" spacing={1.5} alignItems="center">
              {/* Inline marker, not the shared `SearchSpinner` — that one
                  centres itself in a full-width 80px block, which is right
                  for an empty results panel and wrong beside a line of
                  text. */}
              {failure == null && <CircularProgress size={16} thickness={5} />}
              <Typography
                variant="bodySmRegular"
                component="p"
                role="status"
                sx={(theme) => ({
                  color:
                    failure != null
                      ? theme.palette.text.error
                      : theme.palette.text.tertiary,
                })}
              >
                {failure ?? `Resolving ${q.trim()} with ${federatedFor}…`}
              </Typography>
            </Stack>
            {/* A failed resolve otherwise leaves a dead end: no results, no
                next step. The obvious remaining action is the one the input
                shape took away — search for the text itself. */}
            {failure != null && (
              <Box
                component="button"
                type="button"
                onClick={() => setAsText(true)}
                sx={(theme) => ({
                  alignSelf: 'flex-start',
                  background: 'none',
                  border: 'none',
                  p: 0,
                  cursor: 'pointer',
                  textDecoration: 'underline',
                  font: 'inherit',
                  color: theme.palette.text.primary,
                })}
              >
                Search for “{q.trim()}” as text instead
              </Box>
            )}
          </Stack>
        )}
      </Paper>
    </Stack>
  );
}
