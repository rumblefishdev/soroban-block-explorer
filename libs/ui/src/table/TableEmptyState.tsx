import AccountBalanceWalletIcon from '@mui/icons-material/AccountBalanceWalletOutlined';
import AddBoxIcon from '@mui/icons-material/AddBoxOutlined';
import AddCircleIcon from '@mui/icons-material/AddCircleOutline';
import ImageIcon from '@mui/icons-material/ImageOutlined';
import ListAltIcon from '@mui/icons-material/ListAlt';
import { Box, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

export type TableEmptyKind =
  | 'transactions'
  | 'ledgers'
  | 'tokens'
  | 'nft'
  | 'accounts';

interface Preset {
  icon: ReactNode;
  title: string;
  description: string;
}

const PRESETS: Record<TableEmptyKind, Preset> = {
  transactions: {
    icon: <ListAltIcon fontSize="small" />,
    title: 'No transactions yet',
    description: 'Transactions will appear here once network activity begins',
  },
  ledgers: {
    icon: <AddBoxIcon fontSize="small" />,
    title: 'No ledgers indexed',
    description: 'Ledger data will appear once the indexer is running',
  },
  tokens: {
    icon: <AddCircleIcon fontSize="small" />,
    title: 'No tokens found',
    description: 'No tokens have been indexed on this network yet',
  },
  nft: {
    icon: <ImageIcon fontSize="small" />,
    title: 'No NFTs found',
    description: 'No NFT contracts have been deployed on this network yet',
  },
  accounts: {
    icon: <AccountBalanceWalletIcon fontSize="small" />,
    title: 'No accounts found',
    description: 'No accounts have been indexed on this network yet',
  },
};

export interface TableEmptyStateProps {
  kind: TableEmptyKind;
  title?: string;
  description?: string;
}

export function TableEmptyState({
  kind,
  title,
  description,
}: TableEmptyStateProps) {
  const preset = PRESETS[kind];
  return (
    <Stack spacing={1} alignItems="center" sx={{ maxWidth: 240, mx: 'auto' }}>
      <Box
        sx={(theme) => ({
          width: 40,
          height: 40,
          borderRadius: theme.shape.radius.pills,
          backgroundColor: theme.palette.surface.grayMainAlt,
          display: 'inline-flex',
          alignItems: 'center',
          justifyContent: 'center',
          color: theme.palette.text.secondary,
        })}
      >
        {preset.icon}
      </Box>
      <Stack spacing={0.5} alignItems="center" sx={{ textAlign: 'center' }}>
        <Typography variant="bodyRegular" color="text.primary">
          {title ?? preset.title}
        </Typography>
        <Typography variant="bodySmRegular" color="text.secondary">
          {description ?? preset.description}
        </Typography>
      </Stack>
    </Stack>
  );
}
