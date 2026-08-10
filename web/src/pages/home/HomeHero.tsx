import { Box, Stack, Typography } from '@mui/material';
import type { Theme } from '@mui/material/styles';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { routes } from '../../router/routes.js';
import { directRouteFor } from '../../search/directRouteFor.js';

import { HeroSearch } from './HeroSearch.js';

/**
 * Dark mode can simply paint the word in the brand yellow. Light mode cannot —
 * that is 1.26:1 against the page — so the yellow moves behind the word as a
 * highlighter bar and the text itself goes to ink. `isolation` keeps the bar's
 * negative z-index inside this span instead of sliding under the page.
 */
const heroAccentWordSx = (theme: Theme) => {
  const isLight = theme.palette.mode === 'light';
  return {
    position: 'relative',
    display: 'inline-block',
    isolation: 'isolate',
    zIndex: 0,
    color: isLight ? theme.palette.text.primary : theme.palette.text.accent,
    '&::after': {
      content: '""',
      display: isLight ? 'block' : 'none',
      position: 'absolute',
      left: '-0.06em',
      right: '-0.06em',
      bottom: '0.1em',
      height: '0.34em',
      borderRadius: '0.08em',
      backgroundColor: theme.palette.surface.primaryMain,
      zIndex: -1,
    },
  };
};

/**
 * Home page hero — headline, tagline and a large global search input.
 * Submitting the search navigates to the search results page. The
 * background glow + grid live in the page-level backdrop (see `HomePage`).
 */
export function HomeHero() {
  const navigate = useNavigate();
  const [value, setValue] = useState('');

  const submit = () => {
    const q = value.trim();
    if (q) void navigate(directRouteFor(q) ?? routes.search(q));
  };

  return (
    <Box sx={{ pt: { xs: 4, md: 8 }, pb: { xs: 3, md: 5 } }}>
      <Stack spacing={4} alignItems="center">
        <Stack spacing={1.5} alignItems="center">
          <Typography
            variant="heading2SemiBold"
            component="h1"
            sx={{
              textAlign: 'center',
              lineHeight: 1.2,
              fontSize: { xs: 32, sm: 40, md: 48 },
            }}
          >
            <Box component="span" sx={heroAccentWordSx}>
              Soroban
            </Box>{' '}
            - first.
            <br />
            <Box component="span" sx={heroAccentWordSx}>
              Stellar
            </Box>{' '}
            - complete.
          </Typography>
          <Typography
            variant="bodyMedium"
            sx={(theme) => ({
              textAlign: 'center',
              color: theme.palette.text.secondary,
              whiteSpace: { xs: 'normal', md: 'nowrap' },
            })}
          >
            Built for the Soroban era — smart contracts, payments, NFTs,
            liquidity pools, all decoded.
          </Typography>
        </Stack>
        <Box sx={{ width: '100%', maxWidth: 632 }}>
          <HeroSearch value={value} onChange={setValue} onSubmit={submit} />
        </Box>
      </Stack>
    </Box>
  );
}
