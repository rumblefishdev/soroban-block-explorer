import { useState } from 'react';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import SearchIcon from '@mui/icons-material/Search';
import CloseIcon from '@mui/icons-material/Close';
import KeyboardReturnIcon from '@mui/icons-material/KeyboardReturn';

import { monoFontFamily } from '../theme/typography.js';

export type SearchInputSize = 'md' | 'lg';

export interface SearchInputProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit?: () => void;
  onClear?: () => void;
  placeholder?: string;
  size?: SearchInputSize;
}

export function SearchInput({
  value,
  onChange,
  onSubmit,
  onClear,
  placeholder = 'Search by TX hash, accounts, contract, token',
  size = 'md',
}: SearchInputProps) {
  const [focused, setFocused] = useState(false);
  const [hovered, setHovered] = useState(false);

  const isLg = size === 'lg';
  const isActive = focused || value.length > 0;
  const isExpanded = isActive || hovered;

  const height = isLg ? 56 : 40;
  const py = isLg ? 2 : '10px';
  const textVariant = isLg ? 'bodyMedium' : 'bodySmMedium';

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Enter') onSubmit?.();
    if (e.key === 'Escape') {
      onChange('');
      onClear?.();
    }
  }

  return (
    <Box
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      sx={(theme) => ({
        display: 'inline-flex',
        flexDirection: isActive ? 'column' : 'row',
        alignItems: isActive ? 'flex-start' : 'center',
        justifyContent: isActive ? 'center' : undefined,
        width: isExpanded ? 628 : 350,
        height,
        px: 1.5,
        py,
        borderRadius: 1,
        border: `1px solid ${
          isActive
            ? theme.palette.stroke.action
            : hovered
            ? theme.palette.stroke.defaultHover
            : theme.palette.stroke.default
        }`,
        backgroundColor: theme.palette.surface.grayMain,
        gap: isActive ? 0 : hovered ? '4px' : 1,
        transition: 'width 0.2s ease, border-color 0.15s',
        overflow: 'hidden',
        boxSizing: 'border-box',
      })}
    >
      {isActive ? (
        <Box
          sx={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            width: '100%',
          }}
        >
          <Box
            sx={{
              display: 'flex',
              flex: 1,
              alignItems: 'center',
              gap: 1,
              minWidth: 0,
            }}
          >
            <SearchIcon
              sx={(theme) => ({
                width: 16,
                height: 16,
                flexShrink: 0,
                color: theme.palette.text.secondary,
              })}
            />
            <Box
              component="input"
              value={value}
              onChange={(e) => onChange(e.target.value)}
              onFocus={() => setFocused(true)}
              onBlur={() => setFocused(false)}
              onKeyDown={handleKeyDown}
              autoFocus
              aria-label={placeholder}
              sx={(theme) => ({
                flex: 1,
                border: 'none',
                outline: 'none',
                background: 'transparent',
                fontFamily: theme.typography[textVariant].fontFamily,
                fontSize: theme.typography[textVariant].fontSize,
                fontWeight: theme.typography[textVariant].fontWeight,
                lineHeight: theme.typography[textVariant].lineHeight,
                letterSpacing: theme.typography[textVariant].letterSpacing,
                color: theme.palette.text.primary,
                minWidth: 0,
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
              })}
            />
          </Box>

          <Box display="flex" alignItems="center" gap={0.5} flexShrink={0}>
            {value && (
              <Box
                component="button"
                type="button"
                aria-label="Clear search"
                onClick={() => {
                  onChange('');
                  onClear?.();
                }}
                sx={(theme) => ({
                  display: 'inline-flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  width: 20,
                  height: 20,
                  border: 'none',
                  background: 'none',
                  cursor: 'pointer',
                  borderRadius: 1,
                  p: 0,
                  color: theme.palette.text.secondary,
                })}
              >
                <CloseIcon sx={{ width: 16, height: 16 }} />
              </Box>
            )}
            <Box
              sx={(theme) => ({
                display: 'inline-flex',
                alignItems: 'center',
                justifyContent: 'center',
                width: 24,
                height: 24,
                borderRadius: 1,
                backgroundColor: theme.palette.surface.grayLight,
                color: theme.palette.text.tertiary,
              })}
            >
              <KeyboardReturnIcon sx={{ width: 16, height: 16 }} />
            </Box>
          </Box>
        </Box>
      ) : (
        <>
          <Box
            sx={{
              display: 'flex',
              flex: 1,
              alignItems: 'center',
              gap: 1,
              minWidth: 0,
              overflow: 'hidden',
            }}
          >
            <SearchIcon
              sx={(theme) => ({
                width: 16,
                height: 16,
                flexShrink: 0,
                color: theme.palette.text.secondary,
              })}
            />
            <Box
              component="input"
              value={value}
              onChange={(e) => onChange(e.target.value)}
              onFocus={() => setFocused(true)}
              onBlur={() => setFocused(false)}
              onKeyDown={handleKeyDown}
              placeholder={placeholder}
              sx={(theme) => ({
                flex: 1,
                border: 'none',
                outline: 'none',
                background: 'transparent',
                fontFamily: theme.typography[textVariant].fontFamily,
                fontSize: theme.typography[textVariant].fontSize,
                fontWeight: theme.typography[textVariant].fontWeight,
                lineHeight: theme.typography[textVariant].lineHeight,
                letterSpacing: theme.typography[textVariant].letterSpacing,
                color: theme.palette.text.secondary,
                '&::placeholder': { color: theme.palette.text.secondary },
                minWidth: 0,
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
              })}
            />
          </Box>

          <Box
            sx={(theme) => ({
              display: 'inline-flex',
              alignItems: 'center',
              justifyContent: 'center',
              height: 20,
              px: '6px',
              borderRadius: 1,
              backgroundColor: theme.palette.surface.grayLight,
              flexShrink: 0,
            })}
          >
            <Typography
              variant="bodyXsMedium"
              color="text.tertiary"
              noWrap
              sx={{ fontFamily: monoFontFamily }}
            >
              CTRL + F
            </Typography>
          </Box>
        </>
      )}
    </Box>
  );
}
