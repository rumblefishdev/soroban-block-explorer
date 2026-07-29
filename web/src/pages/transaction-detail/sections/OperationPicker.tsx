import type { OperationItem } from '@rumblefish/api-types';
import KeyboardArrowRightIcon from '@mui/icons-material/KeyboardArrowRight';
import { Box, Stack, Typography } from '@mui/material';
import { useMemo } from 'react';

import { formatOperationType } from '../../transactions/operationTypes.js';

export type EnrichedOp = OperationItem;

interface OperationPickerProps {
  operations: readonly EnrichedOp[];
  selectedIndex: number;
  onSelect: (index: number) => void;
}

function opNumber(op: EnrichedOp, index: number): number {
  return op.application_order ?? index + 1;
}

function OpAvatar() {
  return (
    <Box
      sx={(theme) => ({
        width: 32,
        height: 32,
        borderRadius: '50%',
        backgroundColor: theme.palette.blue[100],
        color: theme.palette.blue[600],
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        flexShrink: 0,
        fontFamily: theme.typography.fontFamily,
        fontWeight: 600,
        fontSize: 12,
        letterSpacing: 0.5,
      })}
    >
      OP
    </Box>
  );
}

export function OperationPicker({
  operations,
  selectedIndex,
  onSelect,
}: OperationPickerProps) {
  const visible = useMemo(
    () => operations.map((op, index) => ({ op, index })),
    [operations]
  );

  return (
    <Stack spacing={1.5} sx={{ height: '100%', minWidth: 0 }}>
      <Typography variant="heading6SemiBold" component="h3">
        Choose operation
      </Typography>
      <Stack
        component="ul"
        spacing={1}
        sx={{
          listStyle: 'none',
          m: 0,
          p: 0,
          pr: 1,
          maxHeight: 560,
          overflowY: 'auto',
          scrollbarGutter: 'stable',
        }}
      >
        {visible.length === 0 ? (
          <Box sx={{ p: 2 }}>
            <Typography
              variant="bodySmRegular"
              sx={(theme) => ({ color: theme.palette.text.tertiary })}
            >
              No operations in this transaction.
            </Typography>
          </Box>
        ) : (
          visible.map(({ op, index }) => {
            const selected = index === selectedIndex;
            return (
              <Box
                key={op.appearance_id}
                component="li"
                sx={(theme) => ({
                  display: 'flex',
                  alignItems: 'center',
                  gap: 1.5,
                  p: 1.25,
                  border: `1px solid ${
                    selected ? theme.palette.surface.primaryMain : 'transparent'
                  }`,
                  borderRadius: `${theme.shape.radius.md}px`,
                  backgroundColor: selected
                    ? theme.palette.surface.background
                    : 'transparent',
                  cursor: 'pointer',
                  transition:
                    'border-color 120ms ease, background-color 120ms ease',
                  '&:hover': {
                    backgroundColor: selected
                      ? theme.palette.surface.background
                      : theme.palette.surface.grayHover,
                  },
                })}
                onClick={() => onSelect(index)}
                role="button"
                tabIndex={0}
                aria-pressed={selected}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault();
                    onSelect(index);
                  }
                }}
              >
                <OpAvatar />
                <Stack spacing={0.25} sx={{ minWidth: 0, flex: 1 }}>
                  <Typography
                    variant="bodyMedium"
                    sx={(theme) => ({ color: theme.palette.text.primary })}
                  >
                    {formatOperationType(op.type_name)} #{opNumber(op, index)}
                  </Typography>
                  <Typography
                    variant="bodyXsRegular"
                    sx={(theme) => ({ color: theme.palette.text.tertiary })}
                  >
                    {formatOperationType(op.type_name)}
                  </Typography>
                </Stack>
                <KeyboardArrowRightIcon
                  sx={(theme) => ({
                    fontSize: 18,
                    color: theme.palette.text.tertiary,
                  })}
                />
              </Box>
            );
          })
        )}
      </Stack>
    </Stack>
  );
}
