import { Box, Stack } from '@mui/material';
import { SectionErrorBoundary } from '@rumblefish/soroban-block-explorer-ui';

import { ChainOverview } from './home/ChainOverview.js';
import { HomeHero } from './home/HomeHero.js';
import { LatestLedgers } from './home/LatestLedgers.js';
import { LatestTransactions } from './home/LatestTransactions.js';

/**
 * Home page (`/`) — entry point and chain overview: hero search, chain
 * stats, and the latest transactions and ledgers. Each section has its own
 * error boundary so one failure does not collapse the rest of the page.
 */
export default function HomePage() {
  return (
    // Cancel the AppShell <main> padding so the hero and section
    // backgrounds span the full content width, per the Figma design.
    <Box sx={{ mx: -10, mt: -4 }}>
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
  );
}
