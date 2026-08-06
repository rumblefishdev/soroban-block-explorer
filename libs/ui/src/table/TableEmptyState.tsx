import AccountCircleIcon from '@mui/icons-material/AccountCircleOutlined';
import AddBoxIcon from '@mui/icons-material/AddBoxOutlined';
import AddCircleIcon from '@mui/icons-material/AddCircleOutline';
import CodeIcon from '@mui/icons-material/CodeOutlined';
import ImageIcon from '@mui/icons-material/ImageOutlined';
import ListAltIcon from '@mui/icons-material/ListAlt';
import WaterDropIcon from '@mui/icons-material/WaterDropOutlined';
import { Box, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

export type TableEmptyKind =
  | 'transactions'
  | 'ledgers'
  | 'accounts'
  | 'tokens'
  | 'nft'
  | 'pools'
  | 'contracts';

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
  accounts: {
    icon: <AccountCircleIcon fontSize="small" />,
    title: 'No accounts found',
    description: 'Accounts will appear here once activity is indexed',
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
  pools: {
    icon: <WaterDropIcon fontSize="small" />,
    title: 'No liquidity pools yet',
    description: 'Pools will appear here once liquidity is deployed on chain',
  },
  contracts: {
    icon: <CodeIcon fontSize="small" />,
    title: 'No contracts found',
    description: 'Soroban contracts will appear here once deployed on chain',
  },
};

export interface TableEmptyStateProps {
  kind: TableEmptyKind;
  title?: string;
  description?: string;
  py?: number;
}

export function TableEmptyState({
  kind,
  title,
  description,
  py = 8,
}: TableEmptyStateProps) {
  const preset = PRESETS[kind];
  return (
    <Box sx={{ display: 'flex', justifyContent: 'center', py }}>
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
          <Typography variant="bodyBold" color="text.primary">
            {title ?? preset.title}
          </Typography>
          <Typography variant="bodySmRegular" color="text.secondary">
            {description ?? preset.description}
          </Typography>
        </Stack>
      </Stack>
    </Box>
  );
}
