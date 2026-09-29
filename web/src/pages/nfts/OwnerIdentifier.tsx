import {
  IdentifierDisplay,
  isContractId,
} from '@rumblefish/soroban-block-explorer-ui';

/**
 * An NFT holder, linked to its page: a `G…` account or a `C…` contract (task
 * 0376 — a contract holder used to render as a dash, or as "Burned" on the
 * detail page). The API carries both in one field.
 */
export function OwnerIdentifier({ value }: { value: string }) {
  return (
    <IdentifierDisplay
      value={value}
      type={isContractId(value) ? 'contract' : 'account'}
    />
  );
}
