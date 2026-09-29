import { IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';

/**
 * An NFT holder: an account or a contract (task 0376 — a contract holder used
 * to render as a dash, or as "Burned" on the detail page). Returns `null` when
 * neither is set, so each caller keeps its own empty state.
 */
export function OwnerIdentifier({
  account,
  contract,
}: {
  account: string | null | undefined;
  contract: string | null | undefined;
}) {
  if (account) {
    return <IdentifierDisplay value={account} type="account" />;
  }
  if (contract) {
    return <IdentifierDisplay value={contract} type="contract" />;
  }
  return null;
}
