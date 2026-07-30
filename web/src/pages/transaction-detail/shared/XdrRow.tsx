import ContentCopyIcon from '@mui/icons-material/ContentCopy';
import KeyboardArrowDownIcon from '@mui/icons-material/KeyboardArrowDown';
import KeyboardArrowRightIcon from '@mui/icons-material/KeyboardArrowRight';
import OpenInNewIcon from '@mui/icons-material/OpenInNew';
import {
  Box,
  ButtonBase,
  Collapse,
  IconButton,
  Stack,
  Typography,
} from '@mui/material';
import {
  Chip,
  useCopyToClipboard,
} from '@rumblefish/soroban-block-explorer-ui';
import { useState } from 'react';

interface XdrRowProps {
  label: string;
  value: string;
}

/** Deep link into the official Stellar Lab XDR viewer (lab.stellar.org,
 *  maintained by the SDF) with the blob prefilled. The Lab's state
 *  serializer has no docs — the format was captured from the app itself:
 *  only `/` needs doubling; `+` and `=` pass through raw (verified live
 *  with a crafted blob). */
function stellarLabHref(xdr: string): string {
  return `https://lab.stellar.org/xdr/view?$=xdr$blob=${xdr.replace(
    /\//g,
    '//'
  )};;`;
}

export function XdrRow({ label, value }: XdrRowProps) {
  const [expanded, setExpanded] = useState(false);
  const { copied, copy } = useCopyToClipboard();

  return (
    <Box
      sx={(theme) => ({
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        '&:last-of-type': { borderBottom: 'none' },
      })}
    >
      <Stack
        direction="row"
        alignItems="center"
        justifyContent="space-between"
        spacing={2}
        sx={{ px: 2, py: 1.5, cursor: 'pointer' }}
        onClick={() => setExpanded((v) => !v)}
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
      >
        <Stack
          direction="row"
          spacing={1}
          alignItems="center"
          sx={{ minWidth: 0 }}
        >
          <IconButton
            size="small"
            aria-label={expanded ? `Collapse ${label}` : `Expand ${label}`}
            sx={{ p: 0.25 }}
            onClick={(e) => {
              e.stopPropagation();
              setExpanded((v) => !v);
            }}
          >
            {expanded ? (
              <KeyboardArrowDownIcon fontSize="small" />
            ) : (
              <KeyboardArrowRightIcon fontSize="small" />
            )}
          </IconButton>
          <Typography
            variant="bodyMonoSmRegular"
            sx={(theme) => ({ color: theme.palette.text.primary })}
          >
            {label}
          </Typography>
        </Stack>
        <Stack direction="row" spacing={1} alignItems="center">
          <Typography
            variant="bodySmMedium"
            sx={(theme) => ({ color: theme.palette.text.primary })}
          >
            {value.length} chars
          </Typography>
          <Chip size="sm" color="neutral" label="base64" />
        </Stack>
      </Stack>
      <Collapse in={expanded} unmountOnExit>
        <Box
          sx={(theme) => ({
            mx: 2,
            mb: 1,
            p: 1.5,
            backgroundColor: theme.palette.surface.grayMainAlt,
            border: `1px solid ${theme.palette.stroke.default}`,
            borderRadius: `${theme.shape.radius.s}px`,
          })}
        >
          <Typography
            component="pre"
            variant="bodyMonoSmMedium"
            sx={(theme) => ({
              m: 0,
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-all',
              color: theme.palette.text.tertiary,
            })}
          >
            {value}
          </Typography>
        </Box>
        <Box sx={{ px: 2, pb: 2 }}>
          <ButtonBase
            onClick={() => void copy(value)}
            sx={(theme) => ({
              display: 'inline-flex',
              alignItems: 'center',
              gap: 0.75,
              ...theme.typography.bodySmMedium,
              color: theme.palette.text.primary,
              '&:focus-visible': {
                outline: `2px solid ${theme.palette.stroke.action}`,
                outlineOffset: 2,
                borderRadius: `${theme.shape.radius.xs}px`,
              },
            })}
          >
            <ContentCopyIcon sx={{ fontSize: 14 }} />
            {copied ? 'Copied!' : 'Copy'}
          </ButtonBase>
          <Box
            component="a"
            href={stellarLabHref(value)}
            target="_blank"
            rel="noopener noreferrer"
            sx={(theme) => ({
              display: 'inline-flex',
              alignItems: 'center',
              gap: 0.75,
              ml: 2.5,
              ...theme.typography.bodySmMedium,
              color: theme.palette.text.primary,
              textDecoration: 'none',
              '&:hover': { textDecoration: 'underline' },
              '&:focus-visible': {
                outline: `2px solid ${theme.palette.stroke.action}`,
                outlineOffset: 2,
                borderRadius: `${theme.shape.radius.xs}px`,
              },
            })}
          >
            <OpenInNewIcon sx={{ fontSize: 14 }} />
            Decode in Stellar Lab
          </Box>
        </Box>
      </Collapse>
    </Box>
  );
}
