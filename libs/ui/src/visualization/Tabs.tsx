import { Box, Tab, Tabs as MuiTabs } from '@mui/material';
import type { ReactNode, SyntheticEvent } from 'react';

import { formatInteger } from '../format/index.js';

export interface TabDefinition {
  /** Stable key — also the value synced to the URL by `useTabUrlState`. */
  key: string;
  label: ReactNode;
  /**
   * Optional count rendered as a pill beside the label (Figma: contract
   * detail tabs show e.g. "Invocations 1284").
   */
  count?: number;
  disabled?: boolean;
}

export interface TabsProps {
  tabs: readonly TabDefinition[];
  activeKey: string;
  onChange: (key: string) => void;
  'aria-label'?: string;
}

function TabLabel({
  label,
  count,
  active,
}: {
  label: ReactNode;
  count?: number;
  active: boolean;
}) {
  if (count === undefined) return <>{label}</>;
  return (
    <Box
      component="span"
      sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.75 }}
    >
      {label}
      <Box
        component="span"
        sx={(theme) => ({
          ...theme.typography.bodyXsMedium,
          px: 0.75,
          py: 0.125,
          borderRadius: `${theme.shape.radius.pills}px`,

          backgroundColor: active
            ? theme.palette.surface.primaryMain
            : theme.palette.surface.grayLight,
          color: active
            ? theme.palette.common.black
            : theme.palette.text.secondary,
        })}
      >
        {formatInteger(count)}
      </Box>
    </Box>
  );
}

/**
 * Controlled tab bar built on the explorer-themed MUI Tabs. Keyboard
 * accessible by default (arrow keys move focus, Enter/Space activates).
 * Pair with `useTabUrlState` to reflect the active tab in the URL.
 */
export function Tabs({
  tabs,
  activeKey,
  onChange,
  'aria-label': ariaLabel,
}: TabsProps) {
  return (
    <MuiTabs
      value={activeKey}
      onChange={(_event: SyntheticEvent, key: string) => onChange(key)}
      aria-label={ariaLabel}
      variant="scrollable"
      scrollButtons={false}
    >
      {tabs.map((tab) => (
        <Tab
          key={tab.key}
          value={tab.key}
          disabled={tab.disabled}
          label={
            <TabLabel
              label={tab.label}
              count={tab.count}
              active={tab.key === activeKey}
            />
          }
        />
      ))}
    </MuiTabs>
  );
}
