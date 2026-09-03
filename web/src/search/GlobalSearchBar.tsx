import { Box, ClickAwayListener, Paper, Typography } from '@mui/material';
import { type KeyboardEvent, useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import type { SearchHit } from '@rumblefish/api-types';

import { routes } from '../router/routes.js';

import { routeForHit } from './routeForHit.js';
import { SearchResultsView } from './SearchResultsView.js';
import { useFederatedAddress } from './useFederation.js';
import { useSearchResults } from './useSearchResults.js';

interface GlobalSearchBarProps {
  q: string;
  onDismiss: () => void;

  registerEnterHandler: (handler: () => boolean) => void;
}

export function GlobalSearchBar({
  q,
  onDismiss,
  registerEnterHandler,
}: GlobalSearchBarProps) {
  const navigate = useNavigate();

  // The hook classifies a SEP-2 federated address (`name*domain`) and
  // suppresses its own request for one: `/v1/search` knows nothing about the
  // standard, so its zero hits would render as "No results for
  // karol*lobstr.co" — the one claim that is false, since the results page
  // goes on to resolve it (task 0443).
  const state = useSearchResults({ q });

  // Resolved here rather than only on the results page, so a federated
  // address ends where every other query ends — a row in this dropdown.
  //
  // Nothing leaves the browser until the row is picked. Resolving as you type
  // would dial whatever the half-typed text happens to name, and `lobstr.co`
  // is a real domain on the way to `lobstr.com` (22 such prefix pairs in our
  // own data, several with different owners). Arming is keyed to the exact
  // query, so editing the text takes it back.
  const [armedFor, setArmedFor] = useState<string | null>(null);
  const armed = armedFor != null && armedFor === q.trim();
  const federated = useFederatedAddress(q, armed);
  const federatedFor = federated.domain;
  const resolved =
    armed && federated.data?.kind === 'resolved'
      ? federated.data.accountId
      : null;
  const failure =
    armed && federated.data?.kind === 'failed' ? federated.data.reason : null;

  const arm = useCallback(() => setArmedFor(q.trim()), [q]);

  // Landing on the account is the point of the row; once the two hops answer,
  // go, rather than making the user click the same row a second time.
  useEffect(() => {
    if (resolved == null) return;
    onDismiss();
    navigate(routes.account(resolved));
  }, [resolved, navigate, onDismiss]);
  const [highlightedIndex, setHighlightedIndex] = useState(-1);

  useEffect(() => {
    setHighlightedIndex(-1);
  }, [state.activeTab, state.effectiveQuery]);

  const selectHitByKeyboard = useCallback(
    (hit: SearchHit) => {
      navigate(routeForHit(hit));
      onDismiss();
    },
    [navigate, onDismiss]
  );

  useEffect(() => {
    registerEnterHandler(() => {
      // Enter is an explicit act, so it arms the lookup — and stays on the
      // page while the two hops run, instead of handing off to /search.
      if (federatedFor != null) {
        arm();
        return true;
      }
      const picked = state.hitsForActiveTab[highlightedIndex];
      if (picked) {
        selectHitByKeyboard(picked);
        return true;
      }
      return false;
    });
  }, [
    registerEnterHandler,
    federatedFor,
    arm,
    state.hitsForActiveTab,
    highlightedIndex,
    selectHitByKeyboard,
  ]);

  const handleKeyDown = useCallback(
    (event: KeyboardEvent<HTMLDivElement>) => {
      const max = state.hitsForActiveTab.length;
      if (event.key === 'ArrowDown') {
        event.preventDefault();
        setHighlightedIndex((prev) => (max === 0 ? -1 : (prev + 1) % max));
        return;
      }
      if (event.key === 'ArrowUp') {
        event.preventDefault();
        setHighlightedIndex((prev) =>
          max === 0 ? -1 : (prev - 1 + max) % max
        );
        return;
      }
      if (event.key === 'Escape') {
        event.preventDefault();
        onDismiss();
      }
    },
    [state.hitsForActiveTab.length, onDismiss]
  );

  const handleClickAway = (event: MouseEvent | TouchEvent) => {
    // A click on the search input itself must NOT dismiss the dropdown — the
    // same click also focuses the input, which re-opens the dropdown; without
    // this guard the click-away would fire on mouse-up and close it again.
    const target = event.target;
    if (target instanceof Element && target.closest('[data-search-input]')) {
      return;
    }
    onDismiss();
  };

  return (
    <ClickAwayListener onClickAway={handleClickAway}>
      <Box onKeyDown={handleKeyDown} role="listbox">
        <Paper
          variant="outlined"
          elevation={0}
          sx={(theme) => ({
            borderColor: theme.palette.stroke.action,
            borderWidth: 1,
            backgroundColor: theme.palette.surface.grayMain,
            overflow: 'hidden',
          })}
        >
          {federatedFor != null ? (
            // A row, not a sentence: this panel is a listbox of clickable
            // results, and a static line would be reachable by Enter only —
            // dead to anyone who got here with the mouse.
            <Box
              component="button"
              type="button"
              role="option"
              aria-selected={false}
              disabled={armed}
              onClick={arm}
              sx={(theme) => ({
                display: 'block',
                width: '100%',
                textAlign: 'left',
                px: 2,
                py: 1.5,
                border: 'none',
                background: 'none',
                cursor: armed ? 'default' : 'pointer',
                '&:hover': {
                  backgroundColor: armed
                    ? 'transparent'
                    : theme.palette.surface.grayHover,
                },
                // Same ring as SearchResultRow — this synthetic row is a
                // button, so it must draw its own.
                '&:focus-visible': {
                  outline: `2px solid ${theme.palette.stroke.action}`,
                  outlineOffset: -2,
                },
              })}
            >
              <Typography variant="bodySmMedium" component="span">
                {q.trim()}
              </Typography>
              <Typography
                variant="bodyXsRegular"
                component="span"
                role="status"
                sx={(theme) => ({
                  display: 'block',
                  color:
                    failure != null
                      ? theme.palette.text.error
                      : theme.palette.text.tertiary,
                })}
              >
                {failure ??
                  (armed
                    ? `Asking ${federatedFor}…`
                    : `Federated address — look it up with ${federatedFor}`)}
              </Typography>
            </Box>
          ) : (
            <SearchResultsView
              state={state}
              highlightedIndex={highlightedIndex}
              onRowMouseEnter={setHighlightedIndex}
              onRowClick={onDismiss}
              maxListHeight={480}
            />
          )}
        </Paper>
      </Box>
    </ClickAwayListener>
  );
}
