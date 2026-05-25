import { isTransactionHash } from '@rumblefish/soroban-block-explorer-ui';
import { useParams } from 'react-router-dom';

export interface TxHashParam {
  hash: string;
  valid: boolean;
}

export function useTxHashParam(): TxHashParam {
  const { hash = '' } = useParams<{ hash: string }>();
  return { hash, valid: isTransactionHash(hash) };
}
