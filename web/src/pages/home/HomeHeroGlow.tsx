import { Box } from '@mui/material';

/**
 * Home-only warm gold glow — two blurred `#fdda24` pills behind the hero.
 * Mounted by AppShell inside the full-bleed backdrop wrapper (next to the
 * page grid) so it spills past the page side margins exactly like the grid,
 * rather than being clipped to the constrained content column. Figma
 * y-coords are offset by the AppShell nav height.
 */
export function HomeHeroGlow() {
  return (
    <Box
      aria-hidden
      sx={(theme) => ({
        position: 'absolute',
        inset: '0 0 auto 0',
        height: 780,
        zIndex: theme.zIndex.pageGlow,
        pointerEvents: 'none',
        overflow: 'hidden',
      })}
    >
      <Box
        sx={(theme) => ({
          position: 'absolute',
          top: 142,
          left: '50%',
          transform: 'translateX(-50%)',
          width: 632,
          height: 110,
          borderRadius: 999,
          backgroundColor: theme.palette.surface.primaryMain,
          opacity: 0.28,
          filter: 'blur(75px)',
        })}
      />
      <Box
        sx={(theme) => ({
          position: 'absolute',
          top: 251,
          left: '50%',
          transform: 'translateX(-50%)',
          width: 1062,
          height: 139,
          borderRadius: 999,
          backgroundColor: theme.palette.surface.primaryMain,
          opacity: 0.28,
          filter: 'blur(75px)',
        })}
      />
    </Box>
  );
}
