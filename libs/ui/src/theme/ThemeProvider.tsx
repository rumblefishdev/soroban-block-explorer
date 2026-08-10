import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

import CssBaseline from '@mui/material/CssBaseline';
import {
  ThemeProvider as MuiThemeProvider,
  type PaletteMode,
} from '@mui/material/styles';

import { createExplorerTheme } from './theme.js';

const STORAGE_KEY = 'soroban-explorer.color-mode';

interface ColorModeContextValue {
  mode: PaletteMode;
  setMode: (mode: PaletteMode) => void;
  toggleMode: () => void;
}

const ColorModeContext = createContext<ColorModeContextValue | null>(null);

function readInitialMode(defaultMode: PaletteMode): PaletteMode {
  if (typeof window === 'undefined') {
    return defaultMode;
  }
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (stored === 'light' || stored === 'dark') {
      return stored;
    }
  } catch {
    // localStorage access blocked (private mode, etc.) — fall through.
  }
  // ponytail: OS preference deliberately ignored — dark is the product default;
  // only an explicit user toggle (stored above) overrides it.
  return defaultMode;
}

interface ExplorerThemeProviderProps {
  defaultMode?: PaletteMode;
  children: ReactNode;
}

export function ExplorerThemeProvider({
  defaultMode = 'dark',
  children,
}: ExplorerThemeProviderProps) {
  const [mode, setModeState] = useState<PaletteMode>(() =>
    readInitialMode(defaultMode)
  );

  // Persist on the action, not on the state. A mount-time write would stamp
  // the product default into storage for visitors who never touched the
  // toggle, and `readInitialMode` treats a stored value as "the user chose
  // this" — which would then pin them to today's default forever.
  const setMode = useCallback((next: PaletteMode) => {
    setModeState(next);
    try {
      window.localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // localStorage blocked (private mode, etc.) — the choice just won't
      // survive a reload.
    }
  }, []);

  const theme = useMemo(() => createExplorerTheme(mode), [mode]);

  const contextValue = useMemo<ColorModeContextValue>(
    () => ({
      mode,
      setMode,
      toggleMode: () => setMode(mode === 'light' ? 'dark' : 'light'),
    }),
    [mode, setMode]
  );

  return (
    <ColorModeContext.Provider value={contextValue}>
      <MuiThemeProvider theme={theme}>
        <CssBaseline />
        {children}
      </MuiThemeProvider>
    </ColorModeContext.Provider>
  );
}

export function useColorMode(): ColorModeContextValue {
  const ctx = useContext(ColorModeContext);
  if (!ctx) {
    throw new Error(
      'useColorMode must be used inside <ExplorerThemeProvider>.'
    );
  }
  return ctx;
}
