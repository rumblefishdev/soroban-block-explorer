import { Box, Stack, Typography } from '@mui/material';
import { Link } from 'react-router-dom';

import type { SearchHit } from '@rumblefish/api-types';
import {
  IdentifierDisplay,
  RelativeTimestamp,
  StatusChip,
} from '@rumblefish/soroban-block-explorer-ui';

import { routeForHit } from './routeForHit.js';

interface SearchResultRowProps {
  hit: SearchHit;
  highlighted?: boolean;
  onMouseEnter?: () => void;
  onClick?: () => void;
}

export function SearchResultRow({
  hit,
  highlighted = false,
  onMouseEnter,
  onClick,
}: SearchResultRowProps) {
  const showRight = hit.successful != null || hit.last_activity_at != null;

  return (
    <Box
      component={Link}
      to={routeForHit(hit)}
      onMouseEnter={onMouseEnter}
      onClick={onClick}
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'flex-start',
        justifyContent: 'space-between',
        gap: 2,
        padding: '12px 16px',
        textDecoration: 'none',
        color: 'inherit',
        // Rows are transparent — the surrounding surface (the `Paper` wrapper
        // in `GlobalSearchBar` / `SearchResultsPage`) provides the background;
        // hover / keyboard-highlight lift to `grayHover`.
        backgroundColor: highlighted
          ? theme.palette.surface.grayHover
          : 'transparent',
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        cursor: 'pointer',
        '&:last-of-type': { borderBottom: 'none' },
        '&:hover': {
          backgroundColor: theme.palette.surface.grayHover,
        },
        '&:focus-visible': {
          outline: `2px solid ${theme.palette.stroke.action}`,
          outlineOffset: -2,
        },
      })}
    >
      <Stack spacing={0.5} sx={{ minWidth: 0, flex: 1 }}>
        {/* No per-row type chip: rows are always scoped by the active
            entity-type tab (SearchResultsView is the only render path), so the
            chip would just repeat the tab label. (task 0348 F16) */}
        <Box sx={{ minWidth: 0, flexShrink: 1 }}>
          <IdentifierDisplay
            value={hit.identifier}
            type={hit.entity_type}
            linked={false}
          />
        </Box>
        {hit.label && hit.label !== hit.identifier && (
          <Typography
            variant="bodySmRegular"
            sx={(theme) => ({
              color: theme.palette.text.tertiary,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
            })}
          >
            {hit.label}
          </Typography>
        )}
      </Stack>

      {showRight && (
        <Stack
          spacing={0.5}
          alignItems="flex-end"
          sx={{ flexShrink: 0, minWidth: 88 }}
        >
          {hit.successful != null && <StatusChip successful={hit.successful} />}
          {hit.last_activity_at != null && (
            <RelativeTimestamp
              timestamp={hit.last_activity_at}
              variant="bodyXsRegular"
            />
          )}
        </Stack>
      )}
    </Box>
  );
}
