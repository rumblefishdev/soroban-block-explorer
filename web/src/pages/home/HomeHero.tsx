import { Box, Stack, Typography } from '@mui/material';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { routes } from '../../router/routes.js';
import { directRouteFor } from '../../search/directRouteFor.js';

import { HeroSearch } from './HeroSearch.js';

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
    <Box sx={{ pt: 8, pb: 5 }}>
      <Stack spacing={4} alignItems="center">
        <Stack spacing={1.5} alignItems="center">
          <Typography
            variant="heading1Bold"
            component="h1"
            sx={{
              textAlign: 'center',
              lineHeight: 1.15,

              fontSize: { xs: 36, sm: 48, md: 60 },
            }}
          >
            <Box component="span" sx={{ color: 'text.accent' }}>
              Soroban
            </Box>{' '}
            - first.
            <br />
            <Box component="span" sx={{ color: 'text.accent' }}>
              Stellar
            </Box>{' '}
            - complete.
          </Typography>
          <Typography
            variant="bodyRegular"
            sx={{
              textAlign: 'center',
              color: 'text.secondary',

              whiteSpace: { xs: 'normal', md: 'nowrap' },
            }}
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
