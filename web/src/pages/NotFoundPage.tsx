import HomeIcon from '@mui/icons-material/Home';
import { Box, Button } from '@mui/material';
import { NotFoundState } from '@rumblefish/soroban-block-explorer-ui';
import { useNavigate } from 'react-router-dom';

import { routes } from '../router/routes.js';

/**
 * Catch-all route (`path: '*'`). Rendered inside the AppShell `<main>`
 * landmark — unlike the root `errorElement` — so an unknown URL keeps the
 * nav, footer, and a single `<h1>` for screen-reader heading navigation.
 */
export default function NotFoundPage() {
  const navigate = useNavigate();
  return (
    <Box sx={{ display: 'flex', justifyContent: 'center', py: 8 }}>
      <NotFoundState
        titleOverride="Page not found"
        action={
          <Button
            variant="contained"
            startIcon={<HomeIcon />}
            onClick={() => void navigate(routes.home)}
          >
            Back to home
          </Button>
        }
      />
    </Box>
  );
}
