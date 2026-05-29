import CloseIcon from '@mui/icons-material/Close';
import ImageIcon from '@mui/icons-material/ImageOutlined';
import { Box, Stack, Typography } from '@mui/material';
import type { NftItem } from '@rumblefish/api-types';
import { IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';
import { useEffect, useState } from 'react';

import { routes } from '../../router/routes.js';

/** Display label for an NFT — its name, or the token id when unnamed. */
export function nftLabel(row: NftItem): string {
  const name = row.name?.trim();
  return name && name.length > 0 ? name : `Token ${row.token_id}`;
}

interface NftNameCellProps {
  row: NftItem;
}

/**
 * The "NFT" column cell — a 40px preview thumbnail beside the NFT name
 * (linked to the detail page) and its token id. The thumbnail has three
 * states: a loaded image, a neutral placeholder when no media URL exists,
 * and an error placeholder when the URL fails to load — in which case the
 * token-id sub-label also flags the broken media.
 */
export function NftNameCell({ row }: NftNameCellProps) {
  const [failed, setFailed] = useState(false);

  // A new row may reuse this slot — reset the failure flag when src changes.
  useEffect(() => {
    setFailed(false);
  }, [row.media_url]);

  const state = !row.media_url ? 'empty' : failed ? 'error' : 'image';

  return (
    <Stack direction="row" spacing={1} alignItems="center">
      <Box
        sx={(theme) => ({
          width: 40,
          height: 40,
          flexShrink: 0,
          borderRadius: `${theme.shape.radius.s}px`,
          overflow: 'hidden',
          display: 'inline-flex',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor:
            state === 'error'
              ? theme.palette.surface.error
              : theme.palette.surface.grayMainAlt,
          color:
            state === 'error'
              ? theme.palette.text.error
              : theme.palette.text.tertiary,
        })}
      >
        {state === 'image' && (
          <Box
            component="img"
            src={row.media_url ?? undefined}
            alt={nftLabel(row)}
            loading="lazy"
            // NFT media is hosted by attacker-controlled contracts — do not
            // leak the explorer URL as a referrer.
            referrerPolicy="no-referrer"
            onError={() => setFailed(true)}
            sx={{ width: '100%', height: '100%', objectFit: 'cover' }}
          />
        )}
        {state === 'empty' && <ImageIcon sx={{ fontSize: 18 }} />}
        {state === 'error' && <CloseIcon sx={{ fontSize: 18 }} />}
      </Box>

      <Stack sx={{ minWidth: 0 }}>
        <IdentifierDisplay
          value={nftLabel(row)}
          type="nft"
          truncate={false}
          href={routes.nft(row.contract_id, row.token_id)}
        />
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({
            color:
              state === 'error'
                ? theme.palette.text.error
                : theme.palette.text.secondary,
          })}
        >
          {state === 'error'
            ? `#${row.token_id} · Image unavailable`
            : `#${row.token_id}`}
        </Typography>
      </Stack>
    </Stack>
  );
}
