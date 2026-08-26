import { Box, Stack, Typography } from '@mui/material';
import type { AccountSigning } from '@rumblefish/api-types';
import { Chip, IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';

import { SectionCard } from '../detail/SectionCard.js';

/** One rendered row: the account's own key, or one entry of the signer list. */
interface SignerRow {
  key: string;
  weight: number;
  /** `true` for the account's own key — it gets the badge and sorts first. */
  master: boolean;
  /** `ed25519` signers are accounts and link; the hash types are not. */
  linked: boolean;
  note?: string;
}

/**
 * The master key FIRST, then the stored signers.
 *
 * The ledger does not put the account's own key in the signer list — it keeps
 * the weight in `master_weight` and leaves the list to the added keys. A page
 * rendering only the list therefore reads 3-of-4 where the chain says 3-of-5,
 * which is why the row is composed here rather than expected from the API
 * (synthesising it server-side is what Horizon does, and this project reads the
 * ledger instead).
 *
 * Weight 0 is a master key DISABLED for good — 703,871 accounts are configured
 * that way, plus 69,576 with no signers at all — so it must never read as an
 * ordinary low-weight signer.
 */
function rows(accountId: string, signing: AccountSigning): SignerRow[] {
  const master: SignerRow = {
    key: accountId,
    weight: signing.master_weight,
    master: true,
    linked: false,
    note: signing.master_weight === 0 ? 'master key — disabled' : 'master key',
  };
  return [
    master,
    ...signing.signers.map((s) => ({
      key: s.key,
      weight: s.weight,
      master: false,
      // Only `ed25519` signers are accounts (`G…`). `preauth_tx` (`T…`) and
      // `hash_x` (`X…`) are not, and linking them would route to an account
      // page that cannot exist.
      linked: s.type === 'ed25519',
    })),
  ];
}

function SignerLine({ row, alt }: { row: SignerRow; alt: boolean }) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 2,
        px: 2,
        py: 1.25,
        backgroundColor: alt
          ? theme.palette.surface.grayMainAlt
          : theme.palette.surface.grayMain,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        '&:last-of-type': { borderBottom: 'none' },
      })}
    >
      <Stack sx={{ minWidth: 0 }}>
        <IdentifierDisplay
          value={row.key}
          type="account"
          linked={row.linked}
          fontSize={14}
        />
        {row.note && (
          <Typography
            component="span"
            sx={(theme) => ({
              fontSize: 12,
              color: theme.palette.text.tertiary,
            })}
          >
            {row.note}
          </Typography>
        )}
      </Stack>
      <Stack direction="row" spacing={1} alignItems="center" flexShrink={0}>
        {row.master && <Chip size="sm" color="blue" label="master" />}
        <Typography
          variant="bodyMedium"
          sx={(theme) => ({ color: theme.palette.text.primary })}
        >
          {row.weight}
        </Typography>
      </Stack>
    </Box>
  );
}

/**
 * Signers and thresholds.
 *
 * `signing == null` needed a claim about WHY, and the checkpoint seed settled
 * it: the seed wrote entry state for every live account, so a missing row now
 * means the account has no live ledger entry — not that our coverage is thin.
 * Probed against the chain (`getLedgerEntries`, `LedgerKey::Account`, both
 * controls passing): 450 of 450 accounts without a row came back ABSENT, over
 * two disjoint key ranges and both sub-populations (merged, and addresses we
 * only ever saw referenced). Zero exceptions.
 *
 * So the ordinary case states the fact plainly, with no alarm — the answer is
 * known, and dressing a known answer as an unknown is its own kind of lie.
 *
 * The WARNING is kept for the one shape that WOULD be a real gap, and it reads
 * CLASSIC holdings only. A classic trustline cannot exist without an
 * `AccountEntry`, so seeing one with no signing configuration means the gap is
 * ours. A Soroban token balance proves nothing of the sort: it lives in the
 * TOKEN contract's storage keyed by address, so it outlives `account_merge` and
 * an address that was never funded can hold one from the start.
 *
 * Measured 2026-08-26: 10,713 accounts reach this branch, and 100% of their
 * live rows are Soroban (`asset_type = 3`) across all eight id slices — every
 * one of them probed ABSENT on chain (350/350, control 100/100 PRESENT), while
 * their token balances probed PRESENT with amounts matching ours exactly
 * (60/60). Reading all holdings therefore fired the alarm 10,713 times on
 * correct data. Restricted to classic it measures 0, and it still catches the
 * regression it exists for — every live account carries a native XLM row
 * (170,115 of 170,115 in a 1/64 slice), so a live account can never slip past.
 *
 * "No entry on the ledger" covers two different histories, and saying only
 * that made a reader ask which one they were looking at. An account that was
 * CLOSED once existed; an address we only ever saw referenced never did. Both
 * end with no signing configuration, and the page can tell them apart —
 * `deleted` is now read off the same lifecycle column (task 0500), so it is
 * trustworthy in a way it was not when this section first shipped without it.
 */
export function AccountSigners({
  accountId,
  signing,
  hasClassicHoldings,
  hasContractHoldings,
  deleted,
}: {
  accountId: string;
  signing: AccountSigning | null | undefined;
  /**
   * Whether the page is showing a live CLASSIC holding (native or credit).
   * Soroban balances are deliberately excluded — they carry no claim about
   * whether an `AccountEntry` exists.
   */
  hasClassicHoldings: boolean;
  /**
   * Whether the page is showing a Soroban token balance. Only used to explain
   * the no-account case: "no account, yet it holds something" is the reading
   * this section provokes, and it has a plain answer.
   */
  hasContractHoldings: boolean;
  /** The account existed and was closed, as opposed to never existing. */
  deleted: boolean;
}) {
  if (signing == null) {
    // Three different histories end here, and they are not interchangeable:
    // an account holding a classic trustline with no configuration (a real
    // contradiction, measured 0, and what a writer regression looks like); one
    // that was closed; and an address that was never an account at all.
    const absent = hasClassicHoldings
      ? {
          label: 'Not indexed',
          color: 'warning' as const,
          text: 'This account holds a classic trustline, which the ledger cannot record without an account entry — yet we have no signing configuration for it. Treat the signer list as unknown, not as absent.',
        }
      : deleted
      ? {
          label: 'Closed',
          color: 'neutral' as const,
          text: 'This account was closed on the ledger. A closed account has no signers.',
        }
      : {
          label: 'No account',
          color: 'neutral' as const,
          // Holding tokens without being an account is not a contradiction to
          // explain away — it is how Soroban works, and the page is already
          // showing the balance right above this card. 1,325 addresses on
          // pubnet are in exactly this state: sequence number 0, no
          // `AccountEntry` on chain, a token balance that matches the chain
          // exactly. Saying only "no account" left the reader to reconcile the
          // two facts unaided.
          text: hasContractHoldings
            ? 'This address is not a Stellar account — the ledger holds no account entry for it, so it has no signers. It can still hold the contract tokens shown above: those live in the token contract’s own storage and need no account.'
            : 'The ledger holds no account for this address — we know it only because other transactions referenced it.',
        };
    return (
      <SectionCard
        title="Signers"
        meta={<Chip size="sm" color={absent.color} label={absent.label} />}
      >
        <Box sx={{ px: 2, py: 3 }}>
          <Typography
            variant="bodyRegular"
            sx={(theme) => ({ color: theme.palette.text.secondary })}
          >
            {absent.text}
          </Typography>
        </Box>
      </SectionCard>
    );
  }

  const list = rows(accountId, signing);
  const totalWeight = list.reduce((sum, r) => sum + r.weight, 0);
  // Nothing can reach any threshold above zero. 69,576 accounts on pubnet are
  // configured this way — a deliberate lock, not a defect, but it must not
  // read as an ordinary account.
  const locked = totalWeight === 0;

  return (
    <SectionCard
      title="Signers"
      meta={
        locked ? (
          <Chip size="sm" color="error" dot label="No usable key" />
        ) : signing.signers.length === 0 ? (
          <Chip size="sm" color="neutral" label="Single signature" />
        ) : (
          <Chip size="sm" color="violet" label="Multisig" />
        )
      }
    >
      {list.map((row, index) => (
        <SignerLine
          key={`${row.key}-${index}`}
          row={row}
          alt={index % 2 === 1}
        />
      ))}
      <Box
        sx={(theme) => ({
          px: 2,
          py: 1.5,
          borderTop: `1px solid ${theme.palette.stroke.default}`,
        })}
      >
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({ color: theme.palette.text.tertiary })}
        >
          {/* The comparison IS the security claim, so both sides are on
              screen: what the keys add up to, and what each operation class
              demands. No single "N of M" — there are three thresholds, and
              collapsing them would pick one and hide two. */}
          Total weight {totalWeight} · thresholds low {signing.threshold_low} ·
          medium {signing.threshold_med} · high {signing.threshold_high}
        </Typography>
      </Box>
    </SectionCard>
  );
}
