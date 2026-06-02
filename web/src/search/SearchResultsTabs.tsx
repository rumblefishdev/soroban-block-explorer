import { Box, Typography } from '@mui/material';

import type { EntityType } from '@rumblefish/api-types';

import { ENTITY_LABEL, TAB_ORDER } from './useSearchResults.js';

interface SearchResultsTabsProps {
  activeTab: EntityType;
  onChange: (next: EntityType) => void;
  counts: Record<EntityType, number>;
}

export function SearchResultsTabs({
  activeTab,
  onChange,
  counts,
}: SearchResultsTabsProps) {
  return (
    <Box
      role="tablist"
      aria-label="Search result categories"
      sx={(theme) => ({
        display: 'flex',
        gap: 0.5,
        padding: '8px',
        overflowX: 'auto',
        borderBottom: `1px solid ${theme.palette.stroke.default}`,

        scrollbarWidth: 'none',
        '&::-webkit-scrollbar': { display: 'none' },
      })}
    >
      {TAB_ORDER.map((t) => {
        const isActive = t === activeTab;
        const count = counts[t];
        const isDisabled = count === 0;
        return (
          <Box
            key={t}
            role="tab"
            aria-selected={isActive}
            aria-disabled={isDisabled || undefined}
            component="button"
            type="button"
            onClick={(e) => {
              if (isDisabled) return;
              onChange(t);

              e.currentTarget.scrollIntoView({
                behavior: 'smooth',
                inline: 'center',
                block: 'nearest',
              });
            }}
            sx={(theme) => ({
              display: 'inline-flex',
              alignItems: 'center',
              gap: 0.75,
              padding: '4px 10px',
              borderRadius: 9999,
              border: 'none',
              cursor: 'pointer',
              opacity: isDisabled ? 0.5 : 1,
              backgroundColor: isActive
                ? theme.palette.surface.primaryMain
                : 'transparent',
              color: isActive
                ? theme.palette.common.black
                : theme.palette.text.secondary,
              ...theme.typography.bodySmMedium,
              whiteSpace: 'nowrap',
              transition: 'background-color 0.15s, color 0.15s, opacity 0.15s',
              '&:hover': isDisabled
                ? undefined
                : {
                    backgroundColor: isActive
                      ? theme.palette.surface.primaryHover
                      : theme.palette.surface.grayHover,
                  },
              '&:focus-visible': {
                outline: `2px solid ${theme.palette.stroke.action}`,
                outlineOffset: 2,
              },
            })}
          >
            <Box component="span">{ENTITY_LABEL[t]}</Box>
            <CountBadge count={count} />
          </Box>
        );
      })}
    </Box>
  );
}

function CountBadge({ count }: { count: number }) {
  return (
    <Box
      sx={(theme) => ({
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        width: 22,
        height: 22,
        borderRadius: '50%',
        backgroundColor: theme.palette.common.black,
        flexShrink: 0,
      })}
    >
      <Typography
        component="span"
        variant="bodyXsMedium"
        sx={(theme) => ({
          color:
            count > 0 ? theme.palette.text.accent : theme.palette.common.white,
          lineHeight: 1,
        })}
      >
        {count}
      </Typography>
    </Box>
  );
}
