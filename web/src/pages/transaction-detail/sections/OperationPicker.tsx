import type { OperationItem } from '@rumblefish/api-types';
import KeyboardArrowRightIcon from '@mui/icons-material/KeyboardArrowRight';
import { Box, Stack, Typography } from '@mui/material';

import { formatOperationType } from '../../transactions/operationTypes.js';

import { OpAvatar } from '../op-card/opIcon.js';

interface OperationPickerProps {
  operations: readonly OperationItem[];
  selectedIndex: number;
  onSelect: (index: number) => void;
}

function opNumber(op: OperationItem, index: number): number {
  return op.application_order ?? index + 1;
}

export function OperationPicker({
  operations,
  selectedIndex,
  onSelect,
}: OperationPickerProps) {
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
        {operations.length === 0 ? (
          <Box sx={{ p: 2 }}>
            <Typography
              variant="bodySmRegular"
              sx={(theme) => ({ color: theme.palette.text.tertiary })}
            >
              No operations in this transaction.
            </Typography>
          </Box>
        ) : (
          operations.map((op, index) => {
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
                <OpAvatar typeName={op.type_name} />
                <Typography
                  variant="bodyMedium"
                  sx={(theme) => ({
                    color: theme.palette.text.primary,
                    minWidth: 0,
                    flex: 1,
                  })}
                >
                  {formatOperationType(op.type_name)} #{opNumber(op, index)}
                </Typography>
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
