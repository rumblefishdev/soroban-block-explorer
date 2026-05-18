import AccountBalanceWalletIcon from '@mui/icons-material/AccountBalanceWalletOutlined';
import { Box, Link, Stack, Typography } from '@mui/material';
import type { AccountBalance } from '@rumblefish/api-types';
import { EmptyState } from '@rumblefish/soroban-block-explorer-ui';
import { Link as RouterLink } from 'react-router-dom';

import { routes } from '../../router/routes.js';
import { SectionCard } from '../detail/SectionCard.js';
import { formatAmount } from '../format.js';
import { AssetIcon } from '../assets/AssetIcon.js';

function isNative(balance: AccountBalance): boolean {
  return balance.asset_type_name === 'native';
}

function BalanceRow({ balance }: { balance: AccountBalance }) {
  const native = isNative(balance);
  const code = native ? 'XLM' : balance.asset_code ?? '—';
  const typeLabel = native ? 'Native asset' : 'Classic';
  const href =
    !native && balance.asset_code && balance.asset_issuer
      ? routes.asset(`${balance.asset_code}-${balance.asset_issuer}`)
      : undefined;

  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 2,
        p: 2,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        '&:last-of-type': { borderBottom: 'none' },
      })}
    >
      <Stack
        direction="row"
        spacing={1.5}
        alignItems="center"
        sx={{ minWidth: 0 }}
      >
        <AssetIcon code={code} />
        <Box sx={{ minWidth: 0 }}>
          {href ? (
            <Link
              component={RouterLink}
              to={href}
              variant="bodySmMedium"
              sx={{ color: 'text.primary' }}
            >
              {code}
            </Link>
          ) : (
            <Typography variant="bodySmMedium" sx={{ color: 'text.primary' }}>
              {code}
            </Typography>
          )}
          <Typography variant="bodyXsRegular" sx={{ color: 'text.tertiary' }}>
            {typeLabel}
          </Typography>
        </Box>
      </Stack>
      <Box sx={{ textAlign: 'right' }}>
        <Typography variant="bodySmMedium" sx={{ color: 'text.primary' }}>
          {formatAmount(balance.balance, 2)}
        </Typography>
        <Typography variant="bodyXsRegular" sx={{ color: 'text.tertiary' }}>
          {code}
        </Typography>
      </Box>
    </Box>
  );
}

/**
 * Account balances card — the native XLM balance plus every trustline
 * balance. Each non-native code links to its asset detail page.
 */
export function AccountBalances({
  balances,
}: {
  balances: readonly AccountBalance[];
}) {
  const meta = `${balances.length} ${
    balances.length === 1 ? 'asset' : 'assets'
  }`;

  return (
    <SectionCard title="Balances" meta={meta}>
      {balances.length === 0 ? (
        <Box sx={{ display: 'flex', justifyContent: 'center', py: 6 }}>
          <EmptyState
            icon={<AccountBalanceWalletIcon />}
            title="No balances yet"
            description="Balances will appear here once network activity begins"
          />
        </Box>
      ) : (
        balances.map((balance, index) => (
          <BalanceRow
            key={`${balance.asset_code ?? 'native'}-${
              balance.asset_issuer ?? index
            }`}
            balance={balance}
          />
        ))
      )}
    </SectionCard>
  );
}
