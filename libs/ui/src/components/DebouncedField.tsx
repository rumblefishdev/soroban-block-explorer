import SearchIcon from '@mui/icons-material/SearchOutlined';
import { InputAdornment, TextField } from '@mui/material';
import type { ReactNode } from 'react';

import { useDebouncedDraft } from '../hooks/useDebouncedDraft.js';

export interface DebouncedFieldProps {
  value: string;
  placeholder: string;
  ariaLabel: string;
  /** Fixed input width in pixels (or responsive / CSS custom objects). */
  width?: number | string | Record<string, number | string>;
  onCommit: (value: string) => void;
  error?: boolean;
  helperText?: ReactNode;
}

/**
 * A search input that commits its value after a short pause, so filtering
 * does not refetch on every keystroke. Re-syncs when the value changes
 * externally (e.g. a "Clear filters" action).
 */
export function DebouncedField({
  value,
  placeholder,
  ariaLabel,
  width = 320,
  onCommit,
  error,
  helperText,
}: DebouncedFieldProps) {
  const [draft, setDraft] = useDebouncedDraft(value, onCommit);

  return (
    <TextField
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      placeholder={placeholder}
      aria-label={ariaLabel}
      error={error}
      helperText={helperText}
      sx={{
        width: typeof width === 'number' ? { xs: '100%', sm: width } : width,
        maxWidth: '100%',
      }}
      slotProps={{
        input: {
          startAdornment: (
            <InputAdornment position="start">
              <SearchIcon
                sx={(theme) => ({
                  fontSize: 18,
                  color: theme.palette.text.tertiary,
                })}
              />
            </InputAdornment>
          ),
        },
      }}
    />
  );
}
