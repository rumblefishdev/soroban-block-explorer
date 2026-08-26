import AccountBalanceWalletIcon from '@mui/icons-material/AccountBalanceWalletOutlined';
import { Box, Stack, Typography } from '@mui/material';
import type { AccountBalance } from '@rumblefish/api-types';
import {
  Chip,
  EmptyState,
  formatAmount,
  IdentifierDisplay,
  PaginationControls,
  scaleByDecimals,
} from '@rumblefish/soroban-block-explorer-ui';
import { useSearchParams } from 'react-router-dom';

import { PAGE_SIZE } from '../../api/polling.js';

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

/**
 * A row's identity for React, from the asset's own identity rather than its
 * position. One balance row exists per (holder, asset), and an asset is keyed
 * on exactly this four-part tuple, so no two rows can collide — including the
 * two ways a part can be absent (native carries no code or issuer; a classic
 * asset whose issuer we cannot resolve carries no issuer).
 *
 * Position would have worked only per page: with pagination the index restarts
 * at 0 on every page, so page 2's first row would claim page 1's key.
 */
function assetKey(balance: AccountBalance): string {
  return [
    balance.type,
    balance.asset_code ?? '',
    balance.asset_issuer ?? '',
    balance.contract_id ?? '',
  ].join('|');
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
      // XLM's SAC is real, and `/assets` tags it — but a tag earns its place by
      // being RARE, and here it never would be. Only 3,838 of 306,051 asset
      // identities carry a deployed SAC (1.3%), which is what makes the tag
      // worth reading on a classic row; every account holds XLM and XLM always
      // has one, so on this row the tag is a constant and says nothing about
      // the account you are looking at.
      isSac: false,
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
 * The account's assets — native XLM plus every trustline it holds, INCLUDING
 * the ones standing at zero.
 *
 * Called "Assets", not "Balances". A trustline at zero is a real thing the
 * account holds — permission granted to an issuer — and hiding it was issue
 * #377; but a card titled "Balances" listing 3,477 zeros is a name that
 * argues with its own contents. The count says how many assets, and the second
 * clause says how many carry value, so neither number has to be inferred from
 * the other.
 *
 * Ordering is the API's (`BALANCES_SQL`): native pinned, then funded before
 * empty, then size, then recency. It is not re-sorted here — a page boundary
 * has to fall in the same place the server put it.
 */
export function AccountBalances({
  balances,
  deleted,
}: {
  balances: readonly AccountBalance[];
  /** The account was closed — "yet" would promise something that cannot come. */
  deleted: boolean;
}) {
  const funded = balances.filter((b) => b.balance !== '0').length;
  const noun = balances.length === 1 ? 'asset' : 'assets';
  // The second clause only when it says something new: when every asset is
  // funded it restates the first number, and on a one-asset account it reads
  // like bureaucracy.
  const meta =
    funded === balances.length
      ? `${balances.length} ${noun}`
      : `${balances.length} ${noun} · ${funded} with a balance`;

  // Paginated in the browser, not by cursor: the whole set is already on the
  // page, which is what lets the caption state an exact position instead of
  // the "Latest results" the cursor sections have to say. Sliced, never
  // re-sorted — the server decided the order (`BALANCES_SQL`) and a page
  // boundary has to fall where it put it.
  //
  // Position lives in the URL, like every other paginated section here — it
  // survives a reload and can be sent to someone. `?assets=`, not `?cursor=`,
  // because the transactions section below owns that one on the same page.
  // `replace: true` matches `useTableUrlState`: paging is not navigation, and
  // Back should leave the page, not walk it backwards one slice at a time.
  const [params, setParams] = useSearchParams();
  const paged = balances.length > PAGE_SIZE;
  const lastPage = Math.max(0, Math.ceil(balances.length / PAGE_SIZE) - 1);
  // Clamped, not trusted: a pasted `?assets=999`, or the param surviving a
  // move to a smaller account, must show a real page rather than nothing.
  const asked = Number(params.get('assets') ?? '1');
  const page = Number.isSafeInteger(asked)
    ? Math.min(Math.max(asked - 1, 0), lastPage)
    : 0;
  const goTo = (next: number) =>
    setParams(
      (prev) => {
        const p = new URLSearchParams(prev);
        if (next <= 0) p.delete('assets');
        else p.set('assets', String(next + 1));
        return p;
      },
      { replace: true }
    );

  const start = page * PAGE_SIZE;
  const shown = paged ? balances.slice(start, start + PAGE_SIZE) : balances;

  return (
    <SectionCard title="Assets" meta={meta}>
      {balances.length === 0 ? (
        <EmptyState
          icon={<AccountBalanceWalletIcon />}
          title={deleted ? 'No assets' : 'No assets yet'}
          description={
            deleted
              ? 'This account was closed on the ledger.'
              : 'Assets will appear here once network activity begins'
          }
        />
      ) : (
        shown.map((balance, index) => (
          <BalanceRow
            key={assetKey(balance)}
            balance={balance}
            // Alternating row background — every second row.
            alt={index % 2 === 1}
          />
        ))
      )}
      {/* Machinery only when it is earned. At or below one page there is
          nothing to page through, and a five-asset account should show no
          hint that a mechanism exists — 99% of accounts hold 18 or fewer. */}
      {paged && (
        <PaginationControls
          caption={`${start + 1}–${start + shown.length} of ${balances.length}`}
          canPrev={page > 0}
          canNext={page < lastPage}
          onPrev={() => goTo(page - 1)}
          onNext={() => goTo(page + 1)}
        />
      )}
    </SectionCard>
  );
}
