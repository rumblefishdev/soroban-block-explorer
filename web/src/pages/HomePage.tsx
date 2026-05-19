import { Box, Stack } from '@mui/material';
import { alpha } from '@mui/material/styles';
import { SectionErrorBoundary } from '@rumblefish/soroban-block-explorer-ui';

import { ChainOverview } from './home/ChainOverview.js';
import { HomeHero } from './home/HomeHero.js';
import { LatestLedgers } from './home/LatestLedgers.js';
import { LatestTransactions } from './home/LatestTransactions.js';

/**
 * Home page (`/`) — entry point and chain overview: hero search, chain
 * stats, and the latest transactions and ledgers. Each section has its own
 * error boundary so one failure does not collapse the rest of the page.
 *
 * Rendered full-bleed: AppShell drops its content padding for the home
 * route, and each section owns its horizontal padding.
 */
export default function HomePage() {
  return (
    <Box sx={{ position: 'relative' }}>
      {/*
       * Hero backdrop, behind all content — per the Figma home page:
       *  - warm glow = two blurred `#fdda24` pills (`Group 1`: 75px blur,
       *    28% opacity, pill radius), positioned behind the hero/cards;
       *  - a faint ~80px grid, radially masked so it fades at the edges
       *    (`Grid layers`).
       * Figma y-coords are offset by the 88px AppShell nav.
       */}
      <Box
        aria-hidden
        sx={(theme) => ({
          position: 'absolute',
          inset: '0 0 auto 0',
          height: 780,
          zIndex: 0,
          pointerEvents: 'none',
          overflow: 'hidden',
          // Grid lines — exact Figma `verti/hori lines` values: white,
          // 0.11 opacity, 1.26px stroke, 80.69px pitch. The radial mask
          // stands in for the Figma blurred-blob grid mask.
          '&::before': {
            content: '""',
            position: 'absolute',
            inset: 0,
            backgroundImage: `repeating-linear-gradient(0deg, ${alpha(
              theme.palette.common.white,
              0.11
            )} 0 1.26px, transparent 1.26px 80.69px), repeating-linear-gradient(90deg, ${alpha(
              theme.palette.common.white,
              0.11
            )} 0 1.26px, transparent 1.26px 80.69px)`,
            maskImage:
              'radial-gradient(1500px 820px at 50% 240px, #000 0%, transparent 82%)',
            WebkitMaskImage:
              'radial-gradient(1500px 820px at 50% 240px, #000 0%, transparent 82%)',
          },
        })}
      >
        <Box
          sx={(theme) => ({
            position: 'absolute',
            top: 142,
            left: '50%',
            transform: 'translateX(-50%)',
            width: 632,
            height: 110,
            borderRadius: 999,
            backgroundColor: theme.palette.surface.primaryMain,
            opacity: 0.28,
            filter: 'blur(75px)',
          })}
        />
        <Box
          sx={(theme) => ({
            position: 'absolute',
            top: 251,
            left: '50%',
            transform: 'translateX(-50%)',
            width: 1062,
            height: 139,
            borderRadius: 999,
            backgroundColor: theme.palette.surface.primaryMain,
            opacity: 0.28,
            filter: 'blur(75px)',
          })}
        />
      </Box>
      <Box sx={{ position: 'relative', zIndex: 1 }}>
        <HomeHero />
        <Stack spacing={6} sx={{ pb: 4 }}>
          <SectionErrorBoundary sectionName="chain-overview">
            <ChainOverview />
          </SectionErrorBoundary>
          <SectionErrorBoundary sectionName="latest-transactions">
            <LatestTransactions />
          </SectionErrorBoundary>
          <SectionErrorBoundary sectionName="latest-ledgers">
            <LatestLedgers />
          </SectionErrorBoundary>
        </Stack>
      </Box>
    </Box>
  );
}
