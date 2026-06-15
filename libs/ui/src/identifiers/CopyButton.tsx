import CheckIcon from '@mui/icons-material/Check';
import ContentCopyIcon from '@mui/icons-material/ContentCopy';
import IconButton from '@mui/material/IconButton';
import Tooltip from '@mui/material/Tooltip';
import { styled } from '@mui/material/styles';

import { useCopyToClipboard } from '../hooks/useCopyToClipboard.js';

const StyledButton = styled(IconButton, {
  shouldForwardProp: (prop) => prop !== 'copied',
})<{ copied: boolean }>(({ theme, copied }) => ({
  padding: 6,
  borderRadius: 9999,
  width: 26,
  height: 26,
  color: copied ? theme.palette.common.black : theme.palette.text.primary,
  backgroundColor: copied ? theme.palette.surface.primaryMain : 'transparent',
  transition: theme.transitions.create('background-color', {
    duration: theme.transitions.duration.shortest,
  }),
  '&:hover': {
    backgroundColor: copied
      ? theme.palette.surface.primaryMain
      : theme.palette.surface.grayHover,
  },
  '&:focus-visible': {
    outline: `2px solid ${theme.palette.stroke.action}`,
    outlineOffset: 2,
  },
  '& .MuiSvgIcon-root': {
    fontSize: 14,
  },
}));

export interface CopyButtonProps {
  value: string;
  ariaLabel?: string;
}

export function CopyButton({
  value,
  ariaLabel = 'Copy to clipboard',
}: CopyButtonProps) {
  const { copied, copy } = useCopyToClipboard();

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
            borderRadius: `${theme.shape.radius.xs}px`,
            margin: 0,
          }),
        },
      }}
    >
      <StyledButton
        copied={copied}
        aria-label={ariaLabel}
        onClick={(event) => {
          event.stopPropagation();
          void copy(value);
        }}
        size="small"
      >
        {copied ? <CheckIcon /> : <ContentCopyIcon />}
      </StyledButton>
    </Tooltip>
  );
}
