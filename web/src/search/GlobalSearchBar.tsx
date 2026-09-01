import { Box, ClickAwayListener, Paper, Typography } from '@mui/material';
import { type KeyboardEvent, useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import type { SearchHit } from '@rumblefish/api-types';

import { federatedDomain } from './federation.js';
import { routeForHit } from './routeForHit.js';
import { SearchResultsView } from './SearchResultsView.js';
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

  // A SEP-2 federated address (`name*domain`) is not a broad-search query.
  // `/v1/search` knows nothing about the standard, so asking it returns zero
  // hits and the dropdown renders "No results for karol*lobstr.co" — the one
  // claim that is false, since Enter goes on to resolve it. Skip the request
  // and say what Enter will do instead (task 0443 scope A).
  const federatedFor = federatedDomain(q);
  const state = useSearchResults({ q: federatedFor == null ? q : '' });
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
      const picked = state.hitsForActiveTab[highlightedIndex];
      if (picked) {
        selectHitByKeyboard(picked);
        return true;
      }
      return false;
    });
  }, [
    registerEnterHandler,
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
            <Box sx={{ px: 2, py: 1.5 }}>
              <Typography
                variant="bodySmRegular"
                component="p"
                sx={(theme) => ({ color: theme.palette.text.tertiary })}
              >
                Press Enter to resolve {q.trim()} with {federatedFor}
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
