import AccountBalanceWalletIcon from '@mui/icons-material/AccountBalanceWalletOutlined';
import { Box, Link, Stack, Typography } from '@mui/material';
import type { AccountBalance } from '@rumblefish/api-types';
import {
  Chip,
  EmptyState,
  monoFontFamily,
} from '@rumblefish/soroban-block-explorer-ui';
import { Link as RouterLink } from 'react-router-dom';

import { routes } from '../../router/routes.js';
import { SectionCard } from '../detail/SectionCard.js';
import { formatAmount } from '../format.js';
import { AssetIcon, type AssetIconKind } from '../assets/AssetIcon.js';

interface BalanceShape {
  kind: AssetIconKind;
  name: string;
  code: string;
  subline: string;
  chipLabel: 'Classic' | 'SAC' | null;
  href: string | undefined;
}

function shape(balance: AccountBalance): BalanceShape {
  if (balance.asset_type_name === 'native') {
    return {
      kind: 'native',
      name: 'Stellar Lumens',
      code: 'XLM',
      subline: 'Native asset',
      chipLabel: null,
      href: undefined,
    };
  }
  const code = balance.asset_code ?? '—';
  const issuer = balance.asset_issuer ?? '';

  const isSac = issuer.startsWith('C');
  const href =
    balance.asset_code && balance.asset_issuer
      ? routes.asset(`${balance.asset_code}-${balance.asset_issuer}`)
      : undefined;
  return {
    kind: isSac ? 'sac' : 'classic',
    name: code,
    code,
    subline: issuer,
    chipLabel: isSac ? 'SAC' : 'Classic',
    href,
  };
}

function BalanceRow({
  balance,
  alt,
}: {
  balance: AccountBalance;
  alt: boolean;
}) {
  const s = shape(balance);
  const nameNode = s.href ? (
    <Link
      component={RouterLink}
      to={s.href}
      variant="bodyMedium"
      sx={{ color: 'text.primary' }}
    >
      {s.name}
    </Link>
  ) : (
    <Typography variant="bodyMedium" sx={{ color: 'text.primary' }}>
      {s.name}
    </Typography>
  );

  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 2,
        px: 2,
        py: 1,

        backgroundColor: alt
          ? theme.palette.surface.grayMainAlt
          : theme.palette.surface.grayMain,

        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        '&:last-of-type': { borderBottom: 'none' },
      })}
    >
      <Stack
        direction="row"
        spacing={1.5}
        alignItems="center"
        sx={{ minWidth: 0, flex: 1 }}
      >
        <AssetIcon code={s.code} kind={s.kind} size={32} />
        <Box
          sx={{
            display: 'flex',
            flexDirection: 'row',
            gap: 1,
            alignItems: 'center',
          }}
        >
          <Stack>
            {nameNode}
            <Stack
              direction="row"
              spacing={1}
              alignItems="center"
              sx={{ flexWrap: 'wrap', rowGap: 0.5 }}
            >
              <Typography
                component="span"
                sx={(theme) => ({
                  fontFamily: s.kind === 'native' ? undefined : monoFontFamily,
                  fontSize: 12,
                  color: theme.palette.text.tertiary,
                  wordBreak: 'break-all',
                })}
              >
                {s.subline}
              </Typography>
            </Stack>
          </Stack>
          {s.chipLabel && (
            <Chip
              size="sm"
              color={s.chipLabel === 'SAC' ? 'brown' : 'default'}
              label={s.chipLabel}
            />
          )}
        </Box>
      </Stack>
      <Stack sx={{ alignItems: 'flex-end', flexShrink: 0 }} spacing={0.25}>
        <Typography variant="bodyMedium" sx={{ color: 'text.primary' }}>
          {formatAmount(balance.balance, 2)}
        </Typography>
        <Typography variant="bodyXsRegular" sx={{ color: 'text.tertiary' }}>
          {s.code}
        </Typography>
      </Stack>
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
            alt={index % 2 === 1}
          />
        ))
      )}
    </SectionCard>
  );
}
