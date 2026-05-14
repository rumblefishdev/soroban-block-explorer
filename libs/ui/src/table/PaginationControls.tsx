import { Box, Button, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

export interface PaginationControlsProps {
  caption?: ReactNode;
  prevCursor: string | null;
  nextCursor: string | null;
  onPrev?: (cursor: string) => void;
  onNext?: (cursor: string) => void;
}

export function PaginationControls({
  caption,
  prevCursor,
  nextCursor,
  onPrev,
  onNext,
}: PaginationControlsProps) {
  const prevDisabled = !prevCursor;
  const nextDisabled = !nextCursor;
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
        <Button
          variant="text"
          size="small"
          disabled={prevDisabled}
          onClick={() => prevCursor && onPrev?.(prevCursor)}
          sx={(theme) => ({
            borderRadius: theme.shape.radius.pills,
            px: 2,
            py: 1,
            color: prevDisabled
              ? theme.palette.text.tertiary
              : theme.palette.text.primary,
            ...theme.typography.bodySmMedium,
            textTransform: 'none',
          })}
        >
          Previous
        </Button>
        <Button
          variant="text"
          size="small"
          disabled={nextDisabled}
          onClick={() => nextCursor && onNext?.(nextCursor)}
          sx={(theme) => ({
            borderRadius: theme.shape.radius.pills,
            px: 2,
            py: 1,
            color: nextDisabled
              ? theme.palette.text.tertiary
              : theme.palette.text.primary,
            ...theme.typography.bodySmMedium,
            textTransform: 'none',
          })}
        >
          Next
        </Button>
      </Stack>
    </Box>
  );
}
