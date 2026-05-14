import type { ReactNode } from 'react';
import Box from '@mui/material/Box';

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
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        px: 10,
        py: 1,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.grayMain,
        width: '100%',
      })}
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
  );
}
