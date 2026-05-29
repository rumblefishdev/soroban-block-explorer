import { Box, Stack, Typography } from '@mui/material';
import { Link } from 'react-router-dom';

import type { SearchHit } from '@rumblefish/api-types';
import {
  Chip,
  IdentifierDisplay,
  RelativeTimestamp,
  routeForHit,
} from '@rumblefish/soroban-block-explorer-ui';

import { ENTITY_LABEL } from './useSearchResults.js';

interface SearchResultRowProps {
  hit: SearchHit;
  highlighted?: boolean;
  onMouseEnter?: () => void;
  onClick?: () => void;
}

const SUCCESS_CHIP = { color: 'success' as const, label: 'Success' };
const FAILED_CHIP = { color: 'error' as const, label: 'Failed' };

export function SearchResultRow({
  hit,
  highlighted = false,
  onMouseEnter,
  onClick,
}: SearchResultRowProps) {
  const statusChip =
    hit.successful == null ? null : hit.successful ? SUCCESS_CHIP : FAILED_CHIP;
  const showRight = statusChip != null || hit.last_activity_at != null;

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
        <Stack direction="row" spacing={1.5} alignItems="center">
          <Box sx={{ minWidth: 0, flexShrink: 1 }}>
            <IdentifierDisplay
              value={hit.identifier}
              type={hit.entity_type}
              linked={false}
            />
          </Box>
          <Chip
            size="sm"
            color="neutral"
            label={ENTITY_LABEL[hit.entity_type]}
          />
        </Stack>
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
          {statusChip && (
            <Chip
              size="sm"
              dot
              color={statusChip.color}
              label={statusChip.label}
            />
          )}
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
