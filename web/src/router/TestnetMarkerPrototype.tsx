// PROTOTYPE — throwaway (task 0553, PR E). Three ways to mark a testnet page,
// switchable with ?variant=A|B|C and the floating bar at the bottom. Lives on
// branch proto/0553-testnet-marker only; never merged.
import { useEffect, useState, type ReactNode } from 'react';
import { useSearchParams } from 'react-router-dom';
import Box from '@mui/material/Box';
import Menu from '@mui/material/Menu';
import MenuItem from '@mui/material/MenuItem';
import Typography from '@mui/material/Typography';

export type Variant = 'A' | 'B' | 'C';

const VARIANTS: { key: Variant; name: string }[] = [
  { key: 'A', name: 'Pasek nad stroną' },
  { key: 'B', name: 'Plakietka + przełącznik sieci przy logo' },
  { key: 'C', name: 'Ramka strony + sieć w pasku statystyk' },
];

const MAINNET_URL = 'https://sorobanscan.rumblefish.dev';

// Navigation drops ?variant; remember the last one so the choice survives
// clicking around the real pages.
let lastVariant: Variant = 'A';

export function usePrototypeVariant(): Variant {
  const [params] = useSearchParams();
  const fromUrl = params.get('variant');
  if (fromUrl === 'A' || fromUrl === 'B' || fromUrl === 'C') {
    lastVariant = fromUrl;
  }
  return lastVariant;
}

// ---------------------------------------------------------------- Variant A
/** A full-width strip above both nav bars: says what testnet is and links out. */
export function VariantABanner() {
  return (
    <Box
      sx={(theme) => ({
        backgroundColor: theme.palette.surface.warning,
        borderBottom: `1px solid ${theme.palette.stroke.warning}`,
        color: theme.palette.text.warning,
        textAlign: 'center',
        py: 0.75,
        px: 2,
      })}
    >
      <Typography variant="bodySmMedium" component="span">
        <strong>TESTNET</strong> — Stellar Testnet data. Funds have no value and
        the network is reset a few times a year.{' '}
        <Box
          component="a"
          href={MAINNET_URL}
          sx={{ color: 'inherit', textDecoration: 'underline' }}
        >
          Go to Mainnet →
        </Box>
      </Typography>
    </Box>
  );
}

// ---------------------------------------------------------------- Variant B
/** A chip glued to the logo that is also the network switcher. */
export function VariantBLogo({ logo }: { logo: ReactNode }) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  return (
    <Box sx={{ display: 'inline-flex', alignItems: 'center', gap: 1 }}>
      {logo}
      <Box
        component="button"
        onClick={(e: React.MouseEvent<HTMLElement>) =>
          setAnchor(e.currentTarget)
        }
        sx={(theme) => ({
          display: 'inline-flex',
          alignItems: 'center',
          gap: 0.5,
          border: `1px solid ${theme.palette.stroke.warning}`,
          backgroundColor: theme.palette.surface.warning,
          color: theme.palette.text.warning,
          borderRadius: '999px',
          px: 1,
          py: 0.25,
          cursor: 'pointer',
          font: 'inherit',
          fontSize: 12,
          fontWeight: 700,
          letterSpacing: '0.06em',
        })}
      >
        TESTNET ▾
      </Box>
      <Menu
        anchorEl={anchor}
        open={anchor !== null}
        onClose={() => setAnchor(null)}
      >
        <MenuItem component="a" href={MAINNET_URL}>
          Mainnet
        </MenuItem>
        <MenuItem selected onClick={() => setAnchor(null)}>
          Testnet ✓
        </MenuItem>
      </Menu>
    </Box>
  );
}

// ---------------------------------------------------------------- Variant C
/** A thin warning frame around the whole viewport, always visible. */
export function VariantCFrame() {
  return (
    <Box
      aria-hidden
      sx={(theme) => ({
        position: 'fixed',
        inset: 0,
        border: `3px solid ${theme.palette.stroke.warning}`,
        pointerEvents: 'none',
        zIndex: theme.zIndex.modal + 1,
      })}
    />
  );
}

/** The network named as the first stat, in the same row as TPS and Ledger. */
export function VariantCNetworkStat() {
  return (
    <>
      <Box display="flex" alignItems="baseline" gap={1} flexShrink={0}>
        <Typography variant="bodySmMedium" color="text.tertiary" noWrap>
          Network
        </Typography>
        <Typography
          variant="bodyMonoSmMedium"
          noWrap
          sx={(theme) => ({ color: theme.palette.text.warning })}
        >
          Testnet
        </Typography>
      </Box>
      <Box
        sx={(theme) => ({
          width: '1px',
          height: '20px',
          backgroundColor: theme.palette.stroke.default,
          flexShrink: 0,
        })}
      />
    </>
  );
}

// ---------------------------------------------------------- Floating switcher
export function PrototypeSwitcher({ current }: { current: Variant }) {
  const [, setParams] = useSearchParams();
  const index = VARIANTS.findIndex((v) => v.key === current);
  const go = (step: number) => {
    const next = VARIANTS[(index + step + VARIANTS.length) % VARIANTS.length];
    if (!next) return;
    setParams(
      (p) => {
        p.set('variant', next.key);
        return p;
      },
      { replace: true }
    );
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t?.closest('input, textarea, [contenteditable]')) return;
      if (e.key === 'ArrowLeft') go(-1);
      if (e.key === 'ArrowRight') go(1);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  if (import.meta.env.PROD) return null;
  const label = VARIANTS[index];
  return (
    <Box
      sx={{
        position: 'fixed',
        bottom: 20,
        left: '50%',
        transform: 'translateX(-50%)',
        zIndex: 99999,
        display: 'flex',
        alignItems: 'center',
        gap: 1.5,
        px: 2,
        py: 1,
        borderRadius: '999px',
        backgroundColor: '#111',
        color: '#fff',
        boxShadow: '0 6px 24px rgba(0,0,0,.35)',
        fontFamily: 'system-ui, sans-serif',
        fontSize: 14,
        userSelect: 'none',
      }}
    >
      <Box component="button" onClick={() => go(-1)} sx={switcherButton}>
        ←
      </Box>
      <span>
        <strong>{label?.key}</strong> — {label?.name}
      </span>
      <Box component="button" onClick={() => go(1)} sx={switcherButton}>
        →
      </Box>
    </Box>
  );
}

const switcherButton = {
  background: 'transparent',
  border: '1px solid #555',
  color: '#fff',
  borderRadius: '999px',
  width: 28,
  height: 28,
  cursor: 'pointer',
  font: 'inherit',
};
