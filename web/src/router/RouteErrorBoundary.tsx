import HomeIcon from '@mui/icons-material/Home';
import { Button, Container, Stack } from '@mui/material';
import {
  isRouteErrorResponse,
  useNavigate,
  useRouteError,
} from 'react-router-dom';

import {
  GenericErrorState,
  NotFoundState,
} from '@rumblefish/soroban-block-explorer-ui';

import { routes } from './routes.js';

export function RouteErrorBoundary() {
  const error = useRouteError();
  const navigate = useNavigate();

  const goHome = () => navigate(routes.home);

  if (isRouteErrorResponse(error) && error.status === 404) {
    return (
      <Container maxWidth="sm" sx={{ py: 6 }}>
        <Stack alignItems="center">
          <NotFoundState
            titleOverride="Page not found"
            action={
              <Button
                variant="contained"
                startIcon={<HomeIcon />}
                onClick={goHome}
              >
                Back to home
              </Button>
            }
          />
        </Stack>
      </Container>
    );
  }

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
