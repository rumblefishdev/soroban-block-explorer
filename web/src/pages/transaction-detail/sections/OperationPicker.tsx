import type { OperationItem } from '@rumblefish/api-types';
import KeyboardArrowRightIcon from '@mui/icons-material/KeyboardArrowRight';
import { Box, Stack, Typography } from '@mui/material';

import { formatOperationType } from '../../transactions/operationTypes.js';
import { humanizeOp } from '../shared/humanizeOp.js';

import type { OperationEntry } from './operationEntries.js';
import { OpAvatar } from '../op-card/opIcon.js';

interface OperationPickerProps {
  entries: readonly OperationEntry[];
  /** Ops without their own source inherit the transaction's (self-detection
   *  inside humanizeOp). */
  txSourceAccount: string | null;
  selectedIndex: number;
  onSelect: (index: number) => void;
}

function opNumber(op: OperationItem, index: number): number {
  return op.application_order ?? index + 1;
}

export function OperationPicker({
  entries,
  txSourceAccount,
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
        {/* No empty state: `OperationsSection` renders `UnavailableSection`
            at zero entries and only mounts the picker above one, so an empty
            list cannot reach here (task 0482). */}
        {entries.map((entry, index) => {
            const op = entry.row;
            const selected = index === selectedIndex;
            // Mini-headline (0460 #2): the same sentence the card shows, so
            // the index answers "which op does what" without clicking through.
            // The template-less fallback ("X processed") would only repeat
            // the type label above it — omit the line instead.
            const sentence = humanizeOp(
              entry.light ?? op,
              entry.heavy,
              txSourceAccount
            );
            const summary =
              sentence === `${formatOperationType(op.type_name)} processed`
                ? null
                : sentence;
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
                {/* Column flex: the typography variants render as inline
                    spans, so without it label + summary flow as one text. */}
                <Box
                  sx={{
                    minWidth: 0,
                    flex: 1,
                    display: 'flex',
                    flexDirection: 'column',
                  }}
                >
                  <Typography
                    variant="bodyMedium"
                    sx={(theme) => ({
                      color: theme.palette.text.primary,
                    })}
                  >
                    {formatOperationType(op.type_name)} #{opNumber(op, index)}
                  </Typography>
                  {summary != null && (
                    <Typography
                      variant="bodyXsRegular"
                      noWrap
                      sx={(theme) => ({
                        color: theme.palette.text.secondary,
                      })}
                    >
                      {summary}
                    </Typography>
                  )}
                </Box>
                <KeyboardArrowRightIcon
                  sx={(theme) => ({
                    fontSize: 18,
                    color: theme.palette.text.tertiary,
                  })}
                />
              </Box>
            );
        })}
      </Stack>
    </Stack>
  );
}
