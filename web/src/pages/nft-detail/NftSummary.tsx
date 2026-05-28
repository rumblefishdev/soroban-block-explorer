import { Box, Card, Typography } from '@mui/material';
import type { NftDetailResponse } from '@rumblefish/api-types';
import {
  IdentifierDisplay,
  IdentifierWithCopy,
  TableSectionHeader,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

interface NftSummaryProps {
  nft: NftDetailResponse;
}

function Row({
  label,
  value,
  last,
}: {
  label: string;
  value: ReactNode;
  last?: boolean;
}) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        flexDirection: { xs: 'column', sm: 'row' },
        borderBottom: last
          ? 'none'
          : `1px solid ${theme.palette.stroke.default}`,
      })}
    >
      <Box
        sx={{
          width: { xs: '100%', sm: 150 },
          flexShrink: 0,
          px: 2,
          pt: 1.5,
          pb: { xs: 0, sm: 1.5 },
        }}
      >
        <Typography variant="bodySmBold" sx={{ color: 'text.primary' }}>
          {label}
        </Typography>
      </Box>
      <Box
        sx={{
          flex: 1,
          minWidth: 0,
          px: 2,
          pt: { xs: 0.5, sm: 1.5 },
          pb: 1.5,
          display: 'flex',
          alignItems: 'center',
        }}
      >
        {value}
      </Box>
    </Box>
  );
}

/** The "Details" card on the NFT detail page — identity key/value table. */
export function NftSummary({ nft }: NftSummaryProps) {
  return (
    <Card>
      <TableSectionHeader title="Details" />
      <Row
        label="Token ID"
        value={
          <Typography variant="bodyMonoSmRegular">#{nft.token_id}</Typography>
        }
      />
      <Row
        label="Contract ID"
        value={<IdentifierWithCopy value={nft.contract_id} type="contract" />}
      />
      <Row
        label="Current owner"
        // `owner_account` is null once an NFT is burned (ADR 0037 §13).
        value={
          nft.owner_account ? (
            <IdentifierDisplay value={nft.owner_account} type="account" />
          ) : (
            <Typography variant="bodySmRegular" sx={{ color: 'text.tertiary' }}>
              Burned
            </Typography>
          )
        }
        last={nft.minted_at_ledger == null}
      />
      {nft.minted_at_ledger != null && (
        <Row
          label="Minted at ledger"
          // Plain Satoshi text per Figma — not a mono/linked identifier.
          value={
            <Typography variant="bodySmMedium">
              {nft.minted_at_ledger.toLocaleString('en-US')}
            </Typography>
          }
          last
        />
      )}
    </Card>
  );
}
