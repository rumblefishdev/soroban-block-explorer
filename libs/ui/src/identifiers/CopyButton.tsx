import { useCallback, useEffect, useRef, useState } from 'react';

import CheckIcon from '@mui/icons-material/Check';
import ContentCopyIcon from '@mui/icons-material/ContentCopy';
import IconButton from '@mui/material/IconButton';
import Tooltip from '@mui/material/Tooltip';
import { styled } from '@mui/material/styles';

const COPIED_DURATION_MS = 1500;
const COPIED_ICON_COLOR = '#000000';

const StyledButton = styled(IconButton, {
  shouldForwardProp: (prop) => prop !== 'copied',
})<{ copied: boolean }>(({ theme, copied }) => ({
  padding: 6,
  borderRadius: 9999,
  width: 26,
  height: 26,
  color: copied ? COPIED_ICON_COLOR : theme.palette.text.primary,
  backgroundColor: copied ? theme.palette.surface.primaryMain : 'transparent',
  transition: theme.transitions.create('background-color', {
    duration: theme.transitions.duration.shortest,
  }),
  ...(copied
    ? {}
    : {
        '&:hover': {
          backgroundColor: theme.palette.surface.grayHover,
        },
      }),
  '&:focus-visible': {
    outline: `2px solid ${theme.palette.stroke.action}`,
    outlineOffset: 2,
  },
  '& .MuiSvgIcon-root': {
    fontSize: 14,
  },
}));

async function copyToClipboard(value: string): Promise<void> {
  if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(value);
    return;
  }
  if (typeof document === 'undefined') return;
  const el = document.createElement('textarea');
  el.value = value;
  el.setAttribute('readonly', '');
  el.style.position = 'fixed';
  el.style.opacity = '0';
  document.body.appendChild(el);
  el.select();
  try {
    document.execCommand('copy');
  } finally {
    document.body.removeChild(el);
  }
}

export interface CopyButtonProps {
  value: string;
  ariaLabel?: string;
}

export function CopyButton({
  value,
  ariaLabel = 'Copy to clipboard',
}: CopyButtonProps) {
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const handleClick = useCallback(
    async (event: React.MouseEvent<HTMLButtonElement>) => {
      event.stopPropagation();
      try {
        await copyToClipboard(value);
      } catch {
        return;
      }
      setCopied(true);
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => setCopied(false), COPIED_DURATION_MS);
    },
    [value]
  );

  return (
    <Tooltip
      title="Copied!"
      open={copied}
      placement="top"
      disableHoverListener
      disableFocusListener
      disableTouchListener
      arrow={false}
      slotProps={{
        popper: {
          modifiers: [{ name: 'offset', options: { offset: [0, 3] } }],
        },
        tooltip: {
          sx: (theme) => ({
            backgroundColor: theme.palette.surface.grayInverted,
            color: theme.palette.text.inverted,
            fontFamily: theme.typography.fontFamily,
            fontSize: 12,
            fontWeight: 500,
            lineHeight: 1.4,
            letterSpacing: '-0.02em',
            paddingX: 1,
            paddingY: 0.5,
            borderRadius: '4px',
            margin: 0,
          }),
        },
      }}
    >
      <StyledButton
        copied={copied}
        aria-label={ariaLabel}
        onClick={handleClick}
        size="small"
      >
        {copied ? <CheckIcon /> : <ContentCopyIcon />}
      </StyledButton>
    </Tooltip>
  );
}
