import { ToggleButton, ToggleButtonGroup } from '@mui/material';

import type { DetailMode } from '../useDetailMode.js';

interface ModeToggleProps {
  mode: DetailMode;
  onChange: (next: DetailMode) => void;
}

export function ModeToggle({ mode, onChange }: ModeToggleProps) {
  return (
    <ToggleButtonGroup
      exclusive
      size="small"
      value={mode}
      onChange={(_event, next: DetailMode | null) => {
        if (next != null) onChange(next);
      }}
      aria-label="Transaction detail mode"
      sx={(theme) => ({
        gap: 0.5,
        p: 0.5,
        backgroundColor: theme.palette.surface.grayMainAlt,
        borderRadius: `${theme.shape.radius.md}px`,
        '& .MuiToggleButtonGroup-grouped': {
          border: 'none',
          borderRadius: `${theme.shape.radius.md}px`,
          '&:not(:first-of-type), &:not(:last-of-type)': {
            borderRadius: `${theme.shape.radius.md}px`,
            marginLeft: 0,
          },
        },
        '& .MuiToggleButton-root': {
          textTransform: 'none',
          px: 2,
          py: 0.25,
          fontSize: 14,
          fontWeight: 500,
          color: theme.palette.text.tertiary,
          backgroundColor: 'transparent',
          '&:hover': {
            backgroundColor: 'transparent',
            color: theme.palette.text.secondary,
          },
          '&.Mui-selected': {
            color: theme.palette.text.primary,
            backgroundColor: theme.palette.surface.background,
            border: `1px solid ${theme.palette.stroke.default}`,
            '&:hover': {
              backgroundColor: theme.palette.surface.background,
            },
          },
        },
      })}
    >
      <ToggleButton value="normal">Normal</ToggleButton>
      <ToggleButton value="advanced">Advanced</ToggleButton>
    </ToggleButtonGroup>
  );
}
