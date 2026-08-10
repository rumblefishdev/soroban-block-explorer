import Box from '@mui/material/Box';
import { alpha } from '@mui/material/styles';

/**
 * Faint grid backdrop rendered behind the page content. Same look as the
 * Figma home page — white in dark mode and graphite at a lower opacity in
 * light mode, with a 1.26px stroke and 80.69px pitch. A radial mask softens
 * it so it reads as a halo behind the top of the page rather than a hard
 * pattern.
 *
 * Lives in `libs/ui` so AppShell can mount it on every route. The home
 * page layers its warm gold glow pills on top (those stay home-only —
 * see `HomePage`). On every other route the grid alone is the
 * backdrop, no glow.
 *
 * Caller is responsible for giving the parent `position: relative` so
 * this absolute overlay sits behind the content.
 */
export function PageGridBackdrop() {
  return (
    <Box
      aria-hidden
      sx={(theme) => ({
        position: 'absolute',
        inset: '0 0 auto 0',
        height: 780,
        zIndex: theme.zIndex.gridBackdrop,
        pointerEvents: 'none',
        overflow: 'hidden',
        backgroundImage: `repeating-linear-gradient(0deg, ${alpha(
          theme.palette.mode === 'light'
            ? theme.palette.text.primary
            : theme.palette.common.white,
          theme.palette.mode === 'light' ? 0.05 : 0.11
        )} 0 1.26px, transparent 1.26px 80.69px), repeating-linear-gradient(90deg, ${alpha(
          theme.palette.mode === 'light'
            ? theme.palette.text.primary
            : theme.palette.common.white,
          theme.palette.mode === 'light' ? 0.05 : 0.11
        )} 0 1.26px, transparent 1.26px 80.69px)`,
        maskImage:
          'radial-gradient(1500px 820px at 50% 240px, #000 0%, transparent 82%)',
        WebkitMaskImage:
          'radial-gradient(1500px 820px at 50% 240px, #000 0%, transparent 82%)',
      })}
    />
  );
}
