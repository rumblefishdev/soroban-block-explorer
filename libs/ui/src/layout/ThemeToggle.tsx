import DarkModeIcon from '@mui/icons-material/DarkModeOutlined';
import LightModeIcon from '@mui/icons-material/LightModeOutlined';
import IconButton from '@mui/material/IconButton';
import Tooltip from '@mui/material/Tooltip';

import { useColorMode } from '../theme/ThemeProvider.js';

/**
 * Light/dark theme switch — an icon button wired to the existing
 * {@link useColorMode} context (`toggleMode`). The whole colour-mode
 * mechanism (persistence + `prefers-color-scheme` default) already lives
 * in `ExplorerThemeProvider`; this is only the visible control (task 0351
 * F19). Shows the icon of the mode it will switch TO — a sun in dark mode,
 * a moon in light mode — the near-universal convention.
 */
export function ThemeToggle() {
  const { mode, toggleMode } = useColorMode();
  const toLabel = mode === 'dark' ? 'light' : 'dark';

  return (
    <Tooltip title={`Switch to ${toLabel} mode`}>
      <IconButton
        onClick={toggleMode}
        aria-label={`Switch to ${toLabel} mode`}
        disableRipple
        sx={(theme) => ({
          flexShrink: 0,
          p: 0.75,
          borderRadius: `${theme.shape.radius.s}px`,
          // Match the muted nav-link treatment (NavButton): tertiary at rest,
          // secondary + subtle surface on hover — not the strong text.primary
          // accent, which read as out of place next to the links.
          color: theme.palette.text.tertiary,
          transition: 'background-color 0.15s, color 0.15s',
          '&:hover': {
            color: theme.palette.text.secondary,
            backgroundColor: theme.palette.surface.background,
          },
        })}
      >
        {mode === 'dark' ? (
          <LightModeIcon sx={{ fontSize: 20 }} />
        ) : (
          <DarkModeIcon sx={{ fontSize: 20 }} />
        )}
      </IconButton>
    </Tooltip>
  );
}
