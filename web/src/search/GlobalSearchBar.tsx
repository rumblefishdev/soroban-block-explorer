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

  // Resolved here rather than only on the results page: every other input in
  // this bar shows what it found once typing settles, and a row saying "press
  // Enter" was the one place that asked for a second step. The hook waits
  // 500 ms instead of the usual 300 — this request goes to a host the typed
  // text chooses, so a pause mid-word must not be enough to dial it.
  const federated = useFederatedAddress(q);
  const federatedFor = federated.domain;
  const resolved =
    federated.data?.kind === 'resolved' ? federated.data.accountId : null;
  const failure =
    federated.data?.kind === 'failed' ? federated.data.reason : null;
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
      if (resolved != null) {
        navigate(routes.account(resolved));
        onDismiss();
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
    resolved,
    navigate,
    onDismiss,
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
            // dead to anyone who got here with the mouse. The row is disabled
            // until there is somewhere to go, so a click mid-resolve cannot
            // land on the wrong page.
            <Box
              component="button"
              type="button"
              role="option"
              aria-selected={false}
              disabled={resolved == null}
              onClick={() => {
                if (resolved == null) return;
                onDismiss();
                navigate(routes.account(resolved));
              }}
              sx={(theme) => ({
                display: 'block',
                width: '100%',
                textAlign: 'left',
                px: 2,
                py: 1.5,
                border: 'none',
                background: 'none',
                cursor: resolved == null ? 'default' : 'pointer',
                '&:hover': {
                  backgroundColor:
                    resolved == null
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
                {resolved ?? q.trim()}
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
                  (resolved != null
                    ? `${q.trim()} · ${federatedFor}`
                    : `Resolving ${q.trim()} with ${federatedFor}…`)}
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
