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
          px: `${grid.desktop.margin}px`,
          py: 1,
        }}
      >
        <Box sx={{ height: 32, display: 'flex', alignItems: 'center' }}>
          {logo}
        </Box>

        <Box
          display="flex"
          alignItems="stretch"
          sx={{ alignSelf: 'stretch' }}
          gap={1}
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
