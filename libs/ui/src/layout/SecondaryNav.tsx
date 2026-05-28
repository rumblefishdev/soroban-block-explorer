import type { ReactNode } from 'react';
import Box from '@mui/material/Box';

import { grid } from '../theme/grid.js';

import { NavButton } from './NavButton.js';

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
  return (
    <Box
      component="nav"
      sx={(theme) => ({
        width: '100%',
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.grayMain,

        position: 'relative',
        zIndex: 2,
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

        <Box
          display="flex"
          alignItems="stretch"
          gap={1}
          sx={{
            alignSelf: 'stretch',
            minWidth: 0,
            overflowX: { xs: 'auto', md: 'visible' },
            scrollbarWidth: 'none',
            '&::-webkit-scrollbar': { display: 'none' },
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
      </Box>
    </Box>
  );
}
