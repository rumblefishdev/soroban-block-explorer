import ErrorOutlineIcon from '@mui/icons-material/ErrorOutlineOutlined';
import ImageIcon from '@mui/icons-material/ImageOutlined';
import { Box, Stack, Typography } from '@mui/material';
import { useEffect, useMemo, useState, type ReactNode } from 'react';

interface NftMediaPreviewProps {
  mediaUrl?: string | null;
  /** NFT label, used as the image alt text. */
  name: string;
}

const IMAGE_EXT = new Set([
  'png',
  'jpg',
  'jpeg',
  'gif',
  'webp',
  'svg',
  'avif',
  'bmp',
]);
const VIDEO_EXT = new Set(['mp4', 'webm', 'ogg', 'ogv', 'mov']);

type MediaKind = 'image' | 'video' | 'unsupported';

/** Lower-cased file extension of a URL path, or `null` when there is none. */
function extensionOf(url: string): string | null {
  try {
    const { pathname } = new URL(url, 'https://placeholder.invalid');
    const dot = pathname.lastIndexOf('.');
    if (dot < 0 || dot === pathname.length - 1) return null;
    return pathname.slice(dot + 1).toLowerCase();
  } catch {
    return null;
  }
}

function classify(url: string): MediaKind {
  const ext = extensionOf(url);
  if (ext && VIDEO_EXT.has(ext)) return 'video';
  if (ext && !IMAGE_EXT.has(ext)) return 'unsupported';
  // Image extension or extension-less (typical IPFS) — render as an image.
  return 'image';
}

const BOX_SIZE = 308;

function Frame({
  children,
  bg,
}: {
  children: ReactNode;
  bg: 'image' | 'empty' | 'error';
}) {
  return (
    <Box
      sx={(theme) => ({
        width: { xs: '100%', md: BOX_SIZE },
        maxWidth: BOX_SIZE,
        aspectRatio: '1 / 1',
        flexShrink: 0,
        borderRadius: `${theme.shape.radius.lg}px`,
        overflow: 'hidden',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        border:
          bg === 'error' ? `1px solid ${theme.palette.stroke.error}` : 'none',
        backgroundColor:
          bg === 'image'
            ? theme.palette.common.white
            : bg === 'error'
            ? theme.palette.surface.error
            : theme.palette.surface.grayMain,
      })}
    >
      {children}
    </Box>
  );
}

/**
 * NFT media preview — a fixed 308px square. Images and video render inline
 * (lazy-loaded); a missing URL shows a neutral "No media" placeholder, and a
 * broken or unsupported URL shows a labelled error placeholder, so the
 * detail page is always usable.
 */
export function NftMediaPreview({ mediaUrl, name }: NftMediaPreviewProps) {
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    setFailed(false);
  }, [mediaUrl]);

  const kind = useMemo(
    () => (mediaUrl ? classify(mediaUrl) : null),
    [mediaUrl]
  );

  if (!mediaUrl) {
    return (
      <Frame bg="empty">
        <Stack
          spacing={1}
          alignItems="center"
          sx={{ color: 'text.tertiary', px: 3, textAlign: 'center' }}
        >
          <Box
            sx={(theme) => ({
              width: 48,
              height: 48,
              borderRadius: '50%',
              display: 'inline-flex',
              alignItems: 'center',
              justifyContent: 'center',
              backgroundColor: theme.palette.surface.background,
              color: theme.palette.text.tertiary,
            })}
          >
            <ImageIcon />
          </Box>
          <Typography variant="bodySmMedium" sx={{ color: 'text.tertiary' }}>
            No media available
          </Typography>
          <Typography variant="bodyXsRegular" sx={{ color: 'text.tertiary' }}>
            There is no image
          </Typography>
        </Stack>
      </Frame>
    );
  }

  if (failed || kind === 'unsupported') {
    return (
      <Frame bg="error">
        <Stack
          spacing={1}
          alignItems="center"
          sx={{ px: 3, textAlign: 'center' }}
        >
          <Box
            sx={(theme) => ({
              width: 48,
              height: 48,
              borderRadius: '50%',
              display: 'inline-flex',
              alignItems: 'center',
              justifyContent: 'center',
              backgroundColor: theme.palette.surface.background,
              color: theme.palette.text.error,
            })}
          >
            <ErrorOutlineIcon />
          </Box>
          <Typography variant="bodySmMedium" sx={{ color: 'text.error' }}>
            Failed to load media
          </Typography>
          <Typography variant="bodyXsRegular" sx={{ color: 'text.secondary' }}>
            The media URL is unavailable or unsupported
          </Typography>
        </Stack>
      </Frame>
    );
  }

  if (kind === 'video') {
    return (
      <Frame bg="image">
        <Box
          component="video"
          src={mediaUrl}
          controls
          preload="metadata"
          onError={() => setFailed(true)}
          sx={{ width: '100%', height: '100%', objectFit: 'contain' }}
        />
      </Frame>
    );
  }

  return (
    <Frame bg="image">
      <Box
        component="img"
        src={mediaUrl}
        alt={name}
        loading="lazy"
        // NFT media is hosted by attacker-controlled contracts — do not leak
        // the explorer URL as a referrer.
        referrerPolicy="no-referrer"
        onError={() => setFailed(true)}
        sx={{ width: '100%', height: '100%', objectFit: 'cover' }}
      />
    </Frame>
  );
}
