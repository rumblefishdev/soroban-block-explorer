import { Stack } from '@mui/material';
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
 * The faint grid + warm gold hero glow are full-bleed backdrops mounted by
 * AppShell (so they spill past the side margins); the home glow is gated to
 * this route there. This component only renders the foreground content.
 */
export default function HomePage() {
  return (
    <>
      <HomeHero />
      <Stack spacing={{ xs: 5, md: 10 }} sx={{ pb: 4 }}>
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
    </>
  );
}
