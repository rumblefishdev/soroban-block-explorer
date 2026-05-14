import { Box, Container, Stack, Typography } from '@mui/material';
import { Link, Outlet } from 'react-router-dom';

import { useColorMode } from '@rumblefish/soroban-block-explorer-ui';

import { NAV_LINKS } from './routes.js';

export function AppShellStub() {
  const { mode, toggleMode } = useColorMode();
  return (
    <Box sx={{ minHeight: '100vh', display: 'flex', flexDirection: 'column' }}>
      <Box
        component="header"
        sx={(theme) => ({
          borderBottom: `1px solid ${theme.palette.stroke.default}`,
          backgroundColor: theme.palette.surface.background,
        })}
      >
        <Container maxWidth="lg">
          <Stack
            direction="row"
            spacing={3}
            alignItems="center"
            justifyContent="space-between"
            sx={{ py: 1.5 }}
          >
            <Stack direction="row" spacing={3} alignItems="center">
              <Typography
                component={Link}
                to="/"
                variant="heading5SemiBold"
                sx={{ color: 'text.primary', textDecoration: 'none' }}
              >
                Soroban Explorer
              </Typography>
              <Box component="nav">
                <Stack direction="row" spacing={2}>
                  {NAV_LINKS.map((link) => (
                    <Typography
                      key={link.to}
                      component={Link}
                      to={link.to}
                      variant="bodySmMedium"
                      sx={{
                        color: 'text.secondary',
                        textDecoration: 'none',
                        '&:hover': { color: 'text.primary' },
                      }}
                    >
                      {link.label}
                    </Typography>
                  ))}
                </Stack>
              </Box>
            </Stack>
            <Box
              component="button"
              type="button"
              onClick={toggleMode}
              sx={(theme) => ({
                cursor: 'pointer',
                background: 'none',
                border: `1px solid ${theme.palette.stroke.default}`,
                borderRadius: 999,
                padding: '4px 12px',
                ...theme.typography.bodyXsMedium,
                color: theme.palette.text.secondary,
              })}
            >
              mode: {mode}
            </Box>
          </Stack>
        </Container>
      </Box>

      <Box component="main" sx={{ flex: 1, py: 4 }}>
        <Container maxWidth="lg">
          <Outlet />
        </Container>
      </Box>
    </Box>
  );
}
