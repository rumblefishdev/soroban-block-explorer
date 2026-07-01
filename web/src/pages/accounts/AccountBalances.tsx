import AccountBalanceWalletIcon from '@mui/icons-material/AccountBalanceWalletOutlined';
import { Box, Stack, Typography } from '@mui/material';
import type { AccountBalance } from '@rumblefish/api-types';
import {
  Chip,
  EmptyState,
  formatAmount,
  IdentifierDisplay,
  scaleByDecimals,
} from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';
import { SectionCard } from '../detail/SectionCard.js';
import { AssetIcon } from '../assets/AssetIcon.js';

interface BalanceShape {
  /** Native XLM — its subline is prose ("Native asset"); non-native sublines
   *  are issuer addresses, rendered mono. */
  isNative: boolean;
  name: string;
  code: string;
  subline: string;
  chipLabel: 'Classic' | 'SAC' | 'Token' | null;
  href: string | undefined;
}

function shape(balance: AccountBalance): BalanceShape {
  if (balance.asset_type_name === 'native') {
    return {
      isNative: true,
      name: 'Stellar Lumens',
      code: 'XLM',
      subline: 'Native asset',
      chipLabel: null,
      href: undefined,
    };
  }
  // Soroban token (type-3): no classic code/issuer — its identity is the token
  // contract + on-chain symbol. Link to the asset detail page by contract id.
  if (balance.type === 3) {
    const symbol = balance.symbol ?? '—';
    return {
      isNative: false,
      name: symbol,
      code: symbol,
      subline: balance.contract_id ?? '',
      chipLabel: 'Token',
      href: balance.contract_id ? routes.asset(balance.contract_id) : undefined,
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
    isNative: false,
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
    <IdentifierDisplay
      value={s.name}
      type="asset"
      truncate={false}
      href={s.href}
      fontSize={16}
    />
  ) : (
    <Typography
      variant="bodyMedium"
      sx={(theme) => ({ color: theme.palette.text.primary })}
    >
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
        <AssetIcon code={s.code} size={32} />
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
              {s.isNative ? (
                <Typography
                  component="span"
                  sx={(theme) => ({
                    fontSize: 12,
                    color: theme.palette.text.tertiary,
                  })}
                >
                  {s.subline}
                </Typography>
              ) : (
                // Issuer address: truncated via the identifier component
                // (the full id is one tap away on the asset/issuer page);
                // `tone='inherit'` adopts the tertiary subline colour.
                <Box
                  component="span"
                  sx={(theme) => ({ color: theme.palette.text.tertiary })}
                >
                  <IdentifierDisplay
                    value={s.subline}
                    type={s.subline.startsWith('C') ? 'contract' : 'account'}
                    tone="inherit"
                    fontSize={12}
                  />
                </Box>
              )}
            </Stack>
          </Stack>
          {s.chipLabel && (
            <Chip
              size="sm"
              color={
                s.chipLabel === 'SAC'
                  ? 'brown'
                  : s.chipLabel === 'Token'
                    ? 'neutral'
                    : 'default'
              }
              label={s.chipLabel}
            />
          )}
        </Box>
      </Stack>
      <Stack sx={{ alignItems: 'flex-end', flexShrink: 0 }} spacing={0.25}>
        <Typography
          variant="bodyMedium"
          sx={(theme) => ({ color: theme.palette.text.primary })}
        >
          {formatAmount(scaleByDecimals(balance.balance, balance.decimals), 2)}
        </Typography>
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({ color: theme.palette.text.tertiary })}
        >
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
        <EmptyState
          icon={<AccountBalanceWalletIcon />}
          title="No balances yet"
          description="Balances will appear here once network activity begins"
        />
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
