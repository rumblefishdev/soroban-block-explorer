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
 * `signing == null` is the case this section exists to render HONESTLY: it
 * means the entry state was never observed, not that the account has no extra
 * signers. A quarter of indexed accounts are in that state, so it gets a
 * WARNING chip — deliberately not the neutral one "Single signature" uses,
 * because an unknown must not be mistakable for a known answer.
 */
export function AccountSigners({
  accountId,
  signing,
  deleted,
}: {
  accountId: string;
  signing: AccountSigning | null | undefined;
  deleted: boolean;
}) {
  if (signing == null) {
    // A merged account has no ledger entry at all, so "not indexed" would be
    // the wrong reason for the same missing data. Say which one it is.
    return (
      <SectionCard
        title="Signers"
        meta={
          deleted ? (
            <Chip size="sm" color="neutral" label="No account" />
          ) : (
            <Chip size="sm" color="warning" label="Not indexed" />
          )
        }
      >
        <Box sx={{ px: 2, py: 3 }}>
          <Typography
            variant="bodyRegular"
            sx={(theme) => ({ color: theme.palette.text.secondary })}
          >
            {deleted
              ? 'The account was removed from the ledger, so it has no signing configuration.'
              : 'We have not observed this account’s signing configuration. This is not the same as the account having no additional signers.'}
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
