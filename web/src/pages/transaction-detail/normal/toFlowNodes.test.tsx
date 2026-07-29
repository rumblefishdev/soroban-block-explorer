import type {
  E3ResponseTransactionDetailLight,
  OperationItem,
} from '@rumblefish/api-types';
import { describe, expect, it } from 'vitest';

import { toFlowNodes } from './toFlowNodes.js';

const SOURCE = 'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM';
const DEST = 'GBMDODAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAPSOIPZ';

function tx(
  partial: Partial<E3ResponseTransactionDetailLight> = {}
): E3ResponseTransactionDetailLight {
  return {
    hash: 'abc',
    ledger_sequence: 1,
    successful: true,
    operation_count: 1,
    created_at: '2026-01-01T00:00:00Z',
    source_account: SOURCE,
    ...partial,
  } as E3ResponseTransactionDetailLight;
}

function light(
  partial: Partial<OperationItem> & { type_name: string }
): OperationItem {
  return {
    appearance_id: 1,
    type: 1,
    application_order: 1,
    ledger_sequence: 1,
    created_at: '2026-01-01T00:00:00Z',
    pool_ids: [],
    source_account: SOURCE,
    ...partial,
  } as OperationItem;
}

describe('toFlowNodes', () => {
  it('states a green verdict on a successful transaction', () => {
    const nodes = toFlowNodes({
      tx: tx({ successful: true }),
      light: light({ type_name: 'PAYMENT', destination_account: DEST }),
      heavy: null,
    });
    const result = nodes.at(-1);
    expect(result?.kind).toBe('result');
    expect(result?.title).toBe('Result · Success');
  });

  it('states a red verdict with the atomicity wording on a failed transaction', () => {
    const nodes = toFlowNodes({
      tx: tx({ successful: false }),
      light: light({ type_name: 'PAYMENT', destination_account: DEST }),
      heavy: null,
    });
    const result = nodes.at(-1);
    expect(result?.kind).toBe('result-failed');
    expect(result?.title).toBe('Result · Failed');
    // The verdict is stated in words, never colour alone (task 0444).
    const summary = result?.summary as {
      props: { children: Array<{ props: { children: string } }> };
    };
    expect(summary.props.children[0].props.children).toBe(
      'Transaction failed — this operation was not applied.'
    );
  });

  it('hangs the destination off the source for classic operations', () => {
    const nodes = toFlowNodes({
      tx: tx(),
      light: light({ type_name: 'PAYMENT', destination_account: DEST }),
      heavy: null,
    });
    expect(nodes[0]?.kind).toBe('account');
    expect(nodes[0]?.children?.[0]?.kind).toBe('destination');
    expect(nodes[0]?.children?.[0]?.identifier?.value).toBe(DEST);
  });

  it('renders the contract root with the called function for invokes', () => {
    const nodes = toFlowNodes({
      tx: tx(),
      light: light({
        type_name: 'INVOKE_HOST_FUNCTION',
        contract_id: 'CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA',
      }),
      heavy: {
        op_type: 'INVOKE_HOST_FUNCTION',
        application_order: 1,
        details: { functionName: 'plant' },
      },
    });
    expect(nodes[0]?.kind).toBe('account');
    expect(nodes[1]?.kind).toBe('contract');
    expect(nodes[1]?.detail).toBe('· plant()');
    expect(nodes[1]?.connectorLabel).toBe('Invoke');
  });
});
