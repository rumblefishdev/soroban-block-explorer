import { Container, Stack } from '@mui/material';
import { useNavigate, useRouteError } from 'react-router-dom';

import { GenericErrorState } from '@rumblefish/soroban-block-explorer-ui';

/**
 * Root `errorElement` — catches genuinely *thrown* render/loader errors and
 * shows a generic, retryable error state. Unknown URLs are NOT handled here:
 * they hit the catch-all `path: '*'` route (`NotFoundPage`), which renders a
 * 404 inside the AppShell `<main>`. This boundary deliberately has no 404
 * special-case (nothing in the app throws route-404 responses).
 */
export function RouteErrorBoundary() {
  const error = useRouteError();
  const navigate = useNavigate();

  const message =
    error instanceof Error ? error.message : 'An unexpected error occurred.';
  return (
    <Container maxWidth="sm" sx={{ py: 6 }}>
      <Stack alignItems="center">
        <GenericErrorState description={message} onRetry={() => navigate(0)} />
      </Stack>
    </Container>
  );
}
