import type { ContractDetailResponse } from '@rumblefish/api-types';

import { routes } from '../../router/routes.js';

import { contractTypeMeta, type ContractTypeMeta } from './contractType.js';
import { sacAssetCode, sacAssetId, sacAssetLabel } from './sacAsset.js';

/**
 * What a contract IS elsewhere in the explorer — its "face" (task 0472).
 *
 * Every contract class has one, and before this the detail header named only
 * the SAC case: a Fungible token contract and an NFT collection with 10k
 * tokens both rendered a bare "Contract" heading, while the LIST showed a type
 * chip for every row. Same asymmetry task 0441 left behind for SACs, one level
 * down.
 *
 * The label only carries the asset code for a SAC, because `sac_asset` is the
 * one identity the contract endpoint puts on the wire — a Fungible's symbol
 * and an NFT collection's name live on other endpoints, and fetching them here
 * would trade an API round-trip for one word. The chip still links, so the
 * name is one click away.
 *
 * `href` is `undefined` only for `Other` (and for a SAC whose facet did not
 * resolve — 2 of ~3.9k on prod): classes with nothing to point at render an
 * unlinked chip rather than a dead link.
 */
export interface ContractFace {
  meta: ContractTypeMeta;
  label: string;
  href: string | undefined;
  /** Hover/aria text — disambiguates a SAC's code by naming its issuer. */
  title: string | undefined;
}

export function contractFace(contract: ContractDetailResponse): ContractFace {
  if (contract.is_sac) {
    const meta: ContractTypeMeta = {
      label: 'Stellar Asset Contract',
      color: 'accent',
    };
    if (!contract.sac_asset) {
      return { meta, label: meta.label, href: undefined, title: undefined };
    }
    return {
      meta,
      label: `${meta.label} · ${sacAssetCode(contract.sac_asset)}`,
      href: routes.asset(sacAssetId(contract.sac_asset)),
      title: sacAssetLabel(contract.sac_asset),
    };
  }

  const meta = contractTypeMeta(contract.contract_type_name);
  switch (contract.contract_type_name) {
    // A SEP-41 token contract IS an asset — `assets` carries a row keyed by
    // the contract's own surrogate and `/assets/{C…}` resolves the StrKey.
    case 'fungible':
      return {
        meta,
        label: meta.label,
        href: routes.asset(contract.contract_id),
        title: 'View this token as an asset',
      };
    // An NFT contract IS a collection — the NFTs list filters by contract.
    case 'nft':
      return {
        meta,
        label: meta.label,
        href: routes.nftsByContract(contract.contract_id),
        title: 'View this collection',
      };
    default:
      return { meta, label: meta.label, href: undefined, title: undefined };
  }
}
