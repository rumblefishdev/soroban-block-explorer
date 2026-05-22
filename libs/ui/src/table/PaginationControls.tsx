import { Box, Button, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

export interface PaginationControlsProps {
  caption?: ReactNode;
  canPrev: boolean;
  canNext: boolean;
  onPrev?: () => void;
  onNext?: () => void;
}

export function PaginationControls({
  caption,
  canPrev,
  canNext,
  onPrev,
  onNext,
}: PaginationControlsProps) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 2,
        px: 2,
        py: 2,
        backgroundColor: theme.palette.surface.grayMainAlt,
        borderTop: `1px solid ${theme.palette.stroke.default}`,
      })}
    >
      {caption ? (
        <Typography variant="bodyRegular" color="text.secondary">
          {caption}
        </Typography>
      ) : (
        <Box />
      )}
      <Stack direction="row" spacing={1} alignItems="center">
        <PagerButton
          label="Previous"
          disabled={!canPrev}
          onClick={() => onPrev?.()}
        />
        <PagerButton
          label="Next"
          disabled={!canNext}
          onClick={() => onNext?.()}
        />
      </Stack>
    </Box>
  );
}

interface PagerButtonProps {
  label: string;
  disabled: boolean;
  onClick: () => void;
}

function PagerButton({ label, disabled, onClick }: PagerButtonProps) {
  return (
    <Button
      variant="text"
      size="small"
      disabled={disabled}
      onClick={onClick}
      sx={(theme) => ({
        borderRadius: theme.shape.radius.pills,
        px: 2,
        py: 1,
        color: disabled
          ? theme.palette.text.tertiary
          : theme.palette.text.primary,
        ...theme.typography.bodySmMedium,
        textTransform: 'none',
      })}
    >
      {label}
    </Button>
  );
}
