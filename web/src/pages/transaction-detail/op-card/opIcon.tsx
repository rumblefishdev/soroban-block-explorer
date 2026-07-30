import { Box } from '@mui/material';
import AccountBalanceWallet from '@mui/icons-material/AccountBalanceWallet';
import AddCircleOutline from '@mui/icons-material/AddCircleOutline';
import CallMerge from '@mui/icons-material/CallMerge';
import Description from '@mui/icons-material/Description';
import FlashOn from '@mui/icons-material/FlashOn';
import Restore from '@mui/icons-material/Restore';
import Security from '@mui/icons-material/Security';
import Send from '@mui/icons-material/Send';
import Storage from '@mui/icons-material/Storage';
import SwapHoriz from '@mui/icons-material/SwapHoriz';
import Timer from '@mui/icons-material/Timer';
import TrendingUp from '@mui/icons-material/TrendingUp';
import Tune from '@mui/icons-material/Tune';
import Undo from '@mui/icons-material/Undo';
import Update from '@mui/icons-material/Update';
import VerifiedUser from '@mui/icons-material/VerifiedUser';
import Waves from '@mui/icons-material/Waves';
import type { ComponentType } from 'react';

/** Operation type → icon (spec D6, closing the 0257 orphan icon spec).
 *  Grouped by what the operation is about, not 1:1 — the icon carries the
 *  scan, the label carries the precision. */
const ICONS: Record<string, ComponentType<{ fontSize?: 'inherit' }>> = {
  PAYMENT: Send,
  PATH_PAYMENT_STRICT_SEND: SwapHoriz,
  PATH_PAYMENT_STRICT_RECEIVE: SwapHoriz,
  CREATE_ACCOUNT: AddCircleOutline,
  ACCOUNT_MERGE: CallMerge,
  CHANGE_TRUST: VerifiedUser,
  ALLOW_TRUST: VerifiedUser,
  SET_TRUST_LINE_FLAGS: VerifiedUser,
  MANAGE_SELL_OFFER: TrendingUp,
  MANAGE_BUY_OFFER: TrendingUp,
  CREATE_PASSIVE_SELL_OFFER: TrendingUp,
  LIQUIDITY_POOL_DEPOSIT: Waves,
  LIQUIDITY_POOL_WITHDRAW: Waves,
  CREATE_CLAIMABLE_BALANCE: AccountBalanceWallet,
  CLAIM_CLAIMABLE_BALANCE: AccountBalanceWallet,
  CLAWBACK_CLAIMABLE_BALANCE: Undo,
  CLAWBACK: Undo,
  BEGIN_SPONSORING_FUTURE_RESERVES: Security,
  END_SPONSORING_FUTURE_RESERVES: Security,
  REVOKE_SPONSORSHIP: Security,
  SET_OPTIONS: Tune,
  MANAGE_DATA: Storage,
  BUMP_SEQUENCE: Update,
  INVOKE_HOST_FUNCTION: FlashOn,
  EXTEND_FOOTPRINT_TTL: Timer,
  RESTORE_FOOTPRINT: Restore,
};

export function OpIcon({ typeName }: { typeName: string }) {
  const Icon = ICONS[typeName] ?? Description;
  return <Icon fontSize="inherit" />;
}

/** The 32px circled per-type icon used by both the picker rows and the card
 *  header. */
export function OpAvatar({ typeName }: { typeName: string }) {
  return (
    <Box
      sx={(theme) => ({
        width: 32,
        height: 32,
        borderRadius: '50%',
        backgroundColor: theme.palette.blue[100],
        color: theme.palette.blue[600],
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        flexShrink: 0,
        fontSize: 16,
      })}
    >
      <OpIcon typeName={typeName} />
    </Box>
  );
}
