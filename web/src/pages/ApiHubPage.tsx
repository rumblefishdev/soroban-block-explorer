import { Box, Card, Stack, Typography } from '@mui/material';
import { contentLinkSx } from '@rumblefish/soroban-block-explorer-ui';

import { network, pricesApiUrlFor } from '../network.js';
import { PageHeader } from './detail/PageHeader.js';

/**
 * The portal hub at `/api/` (task 0660): one entry per product portal served
 * under `/api/<portal>/`. Each portal is its own SPA, so it is linked with a
 * plain anchor, never through the router.
 */
export default function ApiHubPage() {
  return (
    <Stack spacing={3} sx={{ maxWidth: 880, mx: 'auto' }}>
      <PageHeader title="APIs" subtitle="Stellar data APIs by Rumble Fish" />
      <Card component="section" sx={{ p: { xs: 2.5, md: 4 } }}>
        <Stack spacing={1.5}>
          <Typography variant="heading6SemiBold" component="h2">
            Stellar Prices API
          </Typography>
          <Typography variant="bodyRegular" color="text.secondary">
            USD and XLM prices for Stellar assets: current prices, 24h change
            and volume, OHLCV candles and oracle readings.
          </Typography>
          <Box
            component="a"
            href={pricesApiUrlFor(network)}
            sx={(theme) => ({
              typography: 'bodyBold',
              color: 'text.primary',
              alignSelf: 'flex-start',
              ...contentLinkSx(theme),
            })}
          >
            Get a free API key
          </Box>
        </Stack>
      </Card>
    </Stack>
  );
}
