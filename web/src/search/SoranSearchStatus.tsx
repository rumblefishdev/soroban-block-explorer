import { Button, Stack, Typography } from '@mui/material';

import type { SoranLookup } from './useSoranLookup.js';

/** Return null directly so the native view can show its ordinary empty state. */
export function getSoranSearchStatus(lookup: SoranLookup) {
  if (
    !lookup.supported ||
    !lookup.name ||
    lookup.result?.kind === 'resolved' ||
    (lookup.result?.kind === 'failed' && lookup.result.code === 'not_found')
  )
    return null;
  const failure =
    lookup.result?.kind === 'failed' ? lookup.result.reason : null;
  return (
    <Stack spacing={1} alignItems="center" sx={{ p: 3 }}>
      <Typography variant="bodySmRegular" role="status">
        {lookup.isPaused
          ? 'Offline. Reconnect to look up this Soran name.'
          : failure ?? 'Searching…'}
      </Typography>
      {failure && (
        <Button size="small" onClick={lookup.ask} disabled={lookup.isFetching}>
          Try again
        </Button>
      )}
    </Stack>
  );
}
