import { useState, type ReactNode } from 'react';
import Box from '@mui/material/Box';
import Drawer from '@mui/material/Drawer';
import IconButton from '@mui/material/IconButton';
import CloseIcon from '@mui/icons-material/Close';
import MenuIcon from '@mui/icons-material/Menu';

import { grid } from '../theme/grid.js';

import { NavButton } from './NavButton.js';

/** Below this width the inline nav collapses behind a hamburger drawer. */
const NAV_COLLAPSE_BREAKPOINT = 'md';

export interface NavItem {
  label: string;
  href?: string;
}

export interface SecondaryNavProps {
  logo: ReactNode;
  navItems: NavItem[];
  activePage?: string;
  onNavClick?: (item: NavItem) => void;
}

export function SecondaryNav({
  logo,
  navItems,
  activePage,
  onNavClick,
}: SecondaryNavProps) {
  const [menuOpen, setMenuOpen] = useState(false);

  const handleNav = (item: NavItem) => {
    setMenuOpen(false);
    onNavClick?.(item);
  };

  return (
    <Box
      component="nav"
      sx={(theme) => ({
        width: '100%',
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.grayMain,

        position: 'relative',
        zIndex: theme.zIndex.secondaryNav,
      })}
    >
      <Box
        sx={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          width: '100%',
          maxWidth: grid.desktop.maxWidth,
          mx: 'auto',
          px: {
            xs: `${grid.mobile.margin}px`,
            md: `${grid.desktop.margin}px`,
          },
          py: 1,
          gap: 2,
        }}
      >
        <Box
          sx={{
            height: 32,
            display: 'flex',
            alignItems: 'center',
            flexShrink: 0,
          }}
        >
          {logo}
        </Box>

        {/* Inline nav — desktop only (≥md). */}
        <Box
          display="flex"
          alignItems="stretch"
          gap={1}
          sx={{
            alignSelf: 'stretch',
            minWidth: 0,
            display: { xs: 'none', [NAV_COLLAPSE_BREAKPOINT]: 'flex' },
          }}
        >
          {navItems.map((item) => (
            <NavButton
              key={item.label}
              label={item.label}
              active={activePage === item.label}
              href={item.href}
              onClick={onNavClick ? () => onNavClick(item) : undefined}
            />
          ))}
        </Box>

        {/* Hamburger toggle — mobile/tablet only (<md). */}
        <IconButton
          aria-label={
            menuOpen ? 'Close navigation menu' : 'Open navigation menu'
          }
          aria-expanded={menuOpen}
          onClick={() => setMenuOpen((open) => !open)}
          sx={(theme) => ({
            display: { xs: 'inline-flex', [NAV_COLLAPSE_BREAKPOINT]: 'none' },
            // ≥44px touch target (WCAG 2.5.5 / card 11.6).
            width: 44,
            height: 44,
            color: theme.palette.text.primary,
          })}
        >
          {menuOpen ? <CloseIcon /> : <MenuIcon />}
        </IconButton>
      </Box>

      {/* Slim right drawer (<md). Backdrop + Escape close it — no separate
          close button; the same hamburger flips to ✕ while open. */}
      <Drawer
        anchor="right"
        open={menuOpen}
        onClose={() => setMenuOpen(false)}
        slotProps={{
          paper: {
            sx: (theme) => ({
              width: 'min(72vw, 256px)',
              pt: 1,
              backgroundColor: theme.palette.surface.grayMain,
              backgroundImage: 'none',
              borderLeft: `1px solid ${theme.palette.stroke.default}`,
            }),
          },
        }}
      >
        <Box
          component="nav"
          aria-label="Primary"
          sx={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'stretch',
            px: 1,
          }}
        >
          {navItems.map((item) => (
            <NavButton
              key={item.label}
              label={item.label}
              size="lg"
              active={activePage === item.label}
              href={item.href}
              onClick={() => handleNav(item)}
            />
          ))}
        </Box>
      </Drawer>
    </Box>
  );
}
