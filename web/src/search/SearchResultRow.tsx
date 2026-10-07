import { Box, Stack, Typography } from '@mui/material';
import { Link } from 'react-router-dom';

import { isMuxedSoranHit, type ExplorerSearchHit } from './searchHit.js';
import {
  IdentifierDisplay,
  CopyButton,
  RelativeTimestamp,
  StatusChip,
} from '@rumblefish/soroban-block-explorer-ui';

import { routeForHit } from './routeForHit.js';

interface SearchResultRowProps {
  hit: ExplorerSearchHit;
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
  const muxed = isMuxedSoranHit(hit);
  const memo = hit.soran?.memo;
  const showRight = hit.successful != null || hit.last_activity_at != null;

  return (
    <Box
      component={hit.soran ? 'div' : Link}
      {...(!hit.soran && { to: routeForHit(hit), onClick })}
      onMouseEnter={onMouseEnter}
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
        cursor: muxed ? 'default' : 'pointer',
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
        <Stack direction="row" spacing={1} alignItems="flex-start">
          <Box
            component={hit.soran && !muxed ? Link : 'div'}
            {...(hit.soran && !muxed && { to: routeForHit(hit), onClick })}
            aria-label={hit.soran ? hit.identifier : undefined}
            sx={(theme) => ({
              minWidth: 0,
              flex: 1,
              color: 'inherit',
              textDecoration: 'none',
              '&:focus-visible': {
                outline: `2px solid ${theme.palette.stroke.action}`,
                outlineOffset: 2,
              },
            })}
          >
            <Stack spacing={0.5}>
              {/* The active category tab already identifies the result type. */}
              <Box sx={{ minWidth: 0, flexShrink: 1 }}>
                {muxed ? (
                  <Typography
                    variant="bodyMonoSmMedium"
                    sx={{ overflowWrap: 'anywhere', whiteSpace: 'normal' }}
                  >
                    {hit.identifier}
                  </Typography>
                ) : (
                  <IdentifierDisplay
                    value={hit.identifier}
                    type={hit.entity_type}
                    linked={false}
                  />
                )}
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
          </Box>
          {hit.soran && (
            <CopyButton value={hit.identifier} ariaLabel="Copy Soran address" />
          )}
        </Stack>
        {memo && memo.type !== 'none' && (
          <Stack direction="row" spacing={0.5} alignItems="center">
            <Typography
              variant="bodySmRegular"
              sx={{
                minWidth: 0,
                whiteSpace: 'pre-wrap',
                overflowWrap: 'anywhere',
              }}
            >
              Required memo ({memo.type}): {memo.value}
            </Typography>
            <CopyButton
              value={memo.value}
              ariaLabel="Copy required Soran memo"
            />
          </Stack>
        )}
        {muxed && (
          <Typography variant="bodyXsRegular">
            Muxed account — no detail page is available for this address.
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
