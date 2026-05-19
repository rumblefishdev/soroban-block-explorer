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
 * Rendered full-bleed: AppShell drops its content padding for the home
 * route, and each section owns its horizontal padding.
 */
export default function HomePage() {
  return (
    <>
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
    </>
  );
}
