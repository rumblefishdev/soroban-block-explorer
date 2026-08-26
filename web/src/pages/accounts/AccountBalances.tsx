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
import { NATIVE_ASSET_CODE, SAC_TAG } from '../assets/assetType.js';

interface BalanceShape {
  /** Native XLM — its subline is prose ("Native asset"); non-native sublines
   *  are issuer addresses, rendered mono. */
  isNative: boolean;
  name: string;
  code: string;
  subline: string;
  chipLabel: 'Classic credit' | 'Soroban' | null;
  /** Deployed-SAC facet — a SECOND chip beside the type one, never a
   *  replacement for it (ADR 0051).
   *
   *  Driven by the field alone, ungated by asset type, exactly as
   *  `AssetsTable` and `AssetDetailPage` drive it. Native XLM does have a
   *  deployed SAC and is tagged on all three; `asset_sac` holds no type-3
   *  rows at all, so a Soroban token is structurally false rather than
   *  special-cased here.
   *
   *  This used to be guessed from the issuer address starting with `C` —
   *  which `asset_issuer` never is, since it resolves out of `accounts` —
   *  so the badge could not render even once. */
  isSac: boolean;
  /** What the letter avatar gets. Split from `code` (the ticker under the
   *  amount): a symbol-less token shows an em-dash ticker but must NOT get an
   *  em-dash INITIAL — the avatar keeps its honest `?` (finding 11). */
  avatarCode: string | null;
  href: string | undefined;
}

function shape(balance: AccountBalance): BalanceShape {
  if (balance.asset_type_name === 'native') {
    return {
      isNative: true,
      name: 'Stellar Lumens',
      code: NATIVE_ASSET_CODE,
      avatarCode: NATIVE_ASSET_CODE,
      subline: 'Native asset',
      chipLabel: null,
      isSac: balance.sac_deployed,
      href: routes.asset('native'),
    };
  }
  // Soroban token (type-3): no classic code/issuer — its identity is the token
  // contract + on-chain symbol. Link to the asset detail page by contract id.
  if (balance.type === 3) {
    const symbol = balance.symbol ?? '—';
    return {
      isNative: false,
      // Full on-chain name as the title (like /assets), symbol as the ticker
      // under the amount. Fall back to symbol when the token has no name.
      name: balance.name ?? symbol,
      code: symbol,
      avatarCode: balance.symbol ?? null,
      subline: balance.contract_id ?? '',
      // Chip glossary (task 0472): the type axis calls this class "Soroban"
      // (matching the assets list), emerald like every Soroban-token chip —
      // this said "Token" in neutral, a third name and a third colour for
      // the same thing.
      chipLabel: 'Soroban',
      isSac: balance.sac_deployed,
      href: balance.contract_id ? routes.asset(balance.contract_id) : undefined,
    };
  }

  const code = balance.asset_code ?? '—';
  const issuer = balance.asset_issuer ?? '';

  const href =
    balance.asset_code && balance.asset_issuer
      ? routes.asset(`${balance.asset_code}-${balance.asset_issuer}`)
      : undefined;
  return {
    isNative: false,
    name: code,
    code,
    avatarCode: balance.asset_code ?? null,
    subline: issuer,
    chipLabel: 'Classic credit',
    isSac: balance.sac_deployed,
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
        <AssetIcon code={s.avatarCode} size={32} />
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
              color={s.chipLabel === 'Soroban' ? 'emerald' : 'neutral'}
              label={s.chipLabel}
            />
          )}
          {/* Two orthogonal axes (ADR 0051): the type chip above, and the
              SAC property tag here — the same pairing `AssetsTable` renders. */}
          {s.isSac && (
            <Chip size="sm" color={SAC_TAG.color} label={SAC_TAG.label} />
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
