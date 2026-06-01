import {
  createContext,
  useContext,
  useEffect,
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
  if (typeof window.matchMedia === 'function') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches
      ? 'dark'
      : 'light';
  }
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
  const [mode, setMode] = useState<PaletteMode>(() =>
    readInitialMode(defaultMode)
  );

  useEffect(() => {
    try {
      window.localStorage.setItem(STORAGE_KEY, mode);
    } catch {
      // ignore
    }
  }, [mode]);

  const theme = useMemo(() => createExplorerTheme(mode), [mode]);

  const contextValue = useMemo<ColorModeContextValue>(
    () => ({
      mode,
      setMode,
      toggleMode: () => setMode(mode === 'light' ? 'dark' : 'light'),
    }),
    [mode]
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
