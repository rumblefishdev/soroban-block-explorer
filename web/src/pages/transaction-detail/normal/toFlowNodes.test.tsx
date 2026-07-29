import type {
  E3ResponseTransactionDetailLight,
  OperationItem,
} from '@rumblefish/api-types';
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { toFlowNodes } from './toFlowNodes.js';

/** Minimal operation — only the fields `toFlowNodes` reads. */
function light(
  partial: Partial<OperationItem> & { type_name: string }
): OperationItem {
  return {
    appearance_id: 1,
    type: 1,
    application_order: 1,
    ledger_sequence: 100,
    created_at: '2026-01-01T00:00:00Z',
    pool_ids: [],
    ...partial,
  } as OperationItem;
}

function tx(successful: boolean): E3ResponseTransactionDetailLight {
  return {
    successful,
    source_account: 'GC7KQ4HMZ2ZQFOWHW6IIVPQKGNZ3GTZ3FWRQ4TLTQMWLQZJ7HRVQRE66',
  } as E3ResponseTransactionDetailLight;
}

function resultNode(successful: boolean) {
  const nodes = toFlowNodes({
    tx: tx(successful),
    light: light({ type_name: 'CHANGE_TRUST' }),
    heavy: null,
  });
  const node = nodes.find((n) => n.id === 'result');
  expect(node).toBeDefined();
  return node!;
}

// Task 0444. The node is titled "Result" and painted like a verdict, but was
// hardcoded green and described what the operation did rather than whether it
// worked. Two separate reporters read that as a success on a failed
// transaction, so both the colour-driving `kind` and the wording are pinned.
describe('toFlowNodes — result node', () => {
  it('marks the result as a success on a successful transaction', () => {
    const node = resultNode(true);
    expect(node.kind).toBe('result');
    expect(node.title).toBe('Result · Success');
  });

  it('marks the result as failed, in words and in kind, on a failed transaction', () => {
    const node = resultNode(false);
    // `kind` drives the palette; the title carries the verdict so colour is
    // never the only signal.
    expect(node.kind).toBe('result-failed');
    expect(node.title).toBe('Result · Failed');
  });

  it('says the operation was not applied when the transaction failed', () => {
    render(<>{resultNode(false).summary}</>);
    expect(
      screen.getByText(/Transaction failed — this operation was not applied/)
    ).toBeInTheDocument();
    // The operation description survives underneath it.
    expect(screen.getByText(/Change Trust processed/)).toBeInTheDocument();
  });
});
