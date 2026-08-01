import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import { render, screen } from '@testing-library/react';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { describe, expect, it } from 'vitest';

import { OperationCard } from './OperationCard.js';

const DEST = 'GA5XIGAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGKTM';

function light(
  partial: Partial<OperationItem> & { type_name: string }
): OperationItem {
  return {
    appearance_id: 1,
    type: 1,
    application_order: 2,
    ledger_sequence: 1,
    created_at: '2026-01-01T00:00:00Z',
    pool_ids: [],
    ...partial,
  } as OperationItem;
}

function heavyOf(details: Record<string, unknown>): XdrOperationDto {
  return { op_type: 'PAYMENT', application_order: 2, details };
}

function renderCard(props: Partial<Parameters<typeof OperationCard>[0]> = {}) {
  return render(
    <ExplorerThemeProvider>
      <OperationCard
        light={light({ type_name: 'PAYMENT', destination_account: DEST })}
        heavy={heavyOf({ amount: 1_005_000_000, asset: 'native' })}
        applied
        fallbackOrder={1}
        txSourceAccount={null}
        {...props}
      />
    </ExplorerThemeProvider>
  );
}

describe('OperationCard', () => {
  it('renders the headline sentence, order and type label', () => {
    renderCard();
    expect(screen.getByText('Sent 100.5 XLM to GA5X…GKTM')).toBeTruthy();
    expect(screen.getByText('2 · Payment')).toBeTruthy();
  });

  it('labels the card "not applied" and keeps the disclosure on a failed transaction', () => {
    renderCard({ applied: false });
    expect(screen.getByText('not applied')).toBeTruthy();
    expect(
      screen.getByRole('button', { name: /Operation details/ })
    ).toBeTruthy();
  });

  it('links crossed pools from light.pool_ids (task 0305, absorbed)', () => {
    renderCard({
      light: light({
        type_name: 'PATH_PAYMENT_STRICT_SEND',
        destination_account: DEST,
        pool_ids: ['LA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUWDA'],
      }),
    });
    expect(screen.getByText(/Pools crossed · 1/)).toBeTruthy();
    expect(screen.getByText(/LA7Q/)).toBeTruthy();
  });

  it('starts with the raw details collapsed', () => {
    renderCard();
    expect(
      screen
        .getByRole('button', { name: /Operation details/ })
        .getAttribute('aria-expanded')
    ).toBe('false');
  });

  it('shows only the events attributed to this operation via op_index', () => {
    renderCard({
      contractEvents: [
        {
          event_type: 'contract',
          contract_id:
            'CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA',
          topics: [{ type: 'sym', value: 'transfer' }],
          data: {},
          event_index: 0,
          op_index: 1,
        },
        {
          event_type: 'contract',
          contract_id: null,
          topics: [{ type: 'sym', value: 'mint' }],
          data: {},
          event_index: 1,
          op_index: 0,
        },
        {
          event_type: 'contract',
          contract_id: null,
          topics: [],
          data: {},
          event_index: 2,
          op_index: null,
        },
      ],
    });
    // heavy.application_order = 2 → matches op_index 1 only.
    expect(screen.getByText('transfer')).toBeTruthy();
    expect(screen.queryByText('mint')).toBeNull();
  });

  it('renders the execution trace for invoke ops and hides the auth tree', () => {
    renderCard({
      light: light({ type_name: 'INVOKE_HOST_FUNCTION' }),
      heavy: heavyOf({}),
      operationTree: [{ functionName: 'authorized_fn', args: [] }],
      diagnosticEvents: [
        {
          event_type: 'diagnostic',
          contract_id: null,
          topics: [
            { type: 'sym', value: 'fn_call' },
            {
              type: 'bytes',
              value: 'xzT92aatkBMtnTNkRAThGP6Ivts2hpYWmu/CNZihVeg=',
            },
            { type: 'sym', value: 'swap' },
          ],
          // Short args render inline (hybrid header, option C).
          data: {
            type: 'vec',
            value: [
              {
                type: 'address',
                value:
                  'GC4QMEH5CY5HAEZVC2XNTRV2XBPQWUX2WCV3ANU32HBFNCYIKWHGK7XQ',
              },
              { type: 'i128', value: '13171' },
            ],
          },
          event_index: 0,
          op_index: null,
        },
        {
          event_type: 'diagnostic',
          contract_id: null,
          topics: [
            { type: 'sym', value: 'fn_return' },
            { type: 'sym', value: 'swap' },
          ],
          data: { type: 'u128', value: '81404538' },
          event_index: 1,
          op_index: null,
        },
      ],
    });
    expect(
      screen.getAllByText((_, node) =>
        (node?.textContent ?? '').startsWith('Execution trace · 1 call')
      ).length
    ).toBeGreaterThan(0);
    // Literal args render in a nested (secondary-tone) span, so match the
    // whole header row by its text content.
    expect(
      screen.getByText(
        (_, element) => element?.textContent === 'swap(GC4Q…K7XQ, 13171)'
      )
    ).toBeTruthy();
    expect(screen.getByText('→ 81404538')).toBeTruthy();
    // Auth tree is the fallback only — hidden when the trace is present.
    expect(screen.queryByText(/Authorized calls/)).toBeNull();
  });

  it('renders void args as fn() and inlines short vec returns with links', () => {
    renderCard({
      light: light({ type_name: 'INVOKE_HOST_FUNCTION' }),
      heavy: heavyOf({}),
      diagnosticEvents: [
        {
          event_type: 'diagnostic',
          contract_id: null,
          topics: [
            { type: 'sym', value: 'fn_call' },
            {
              type: 'bytes',
              value: 'xzT92aatkBMtnTNkRAThGP6Ivts2hpYWmu/CNZihVeg=',
            },
            { type: 'sym', value: 'get_tokens' },
          ],
          // Void payload = ZERO arguments — never "1 arg".
          data: { type: 'void', value: null },
          event_index: 0,
          op_index: null,
        },
        {
          event_type: 'diagnostic',
          contract_id: null,
          topics: [
            { type: 'sym', value: 'fn_return' },
            { type: 'sym', value: 'get_tokens' },
          ],
          data: {
            type: 'vec',
            value: [
              {
                type: 'address',
                value:
                  'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA',
              },
              {
                type: 'address',
                value:
                  'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75',
              },
            ],
          },
          event_index: 1,
          op_index: null,
        },
      ],
    });
    expect(
      screen.getByText((_, element) => element?.textContent === 'get_tokens()')
    ).toBeTruthy();
    // The returned pair inlines as LINKS, not an opaque ellipsis.
    expect(
      screen.getByRole('link', { name: /CAS3J7GY/ }).getAttribute('href')
    ).toContain('/contracts/CAS3J7GY');
    expect(screen.getByRole('link', { name: /CCW67TSZ/ })).toBeTruthy();
  });

  it('marks only the deepest unfinished call with "stopped here"', () => {
    const call = (name: string, index: number) => ({
      event_type: 'diagnostic',
      contract_id: null,
      topics: [
        { type: 'sym', value: 'fn_call' },
        {
          type: 'bytes',
          value: 'xzT92aatkBMtnTNkRAThGP6Ivts2hpYWmu/CNZihVeg=',
        },
        { type: 'sym', value: name },
      ],
      data: null,
      event_index: index,
      op_index: null,
    });
    renderCard({
      light: light({ type_name: 'INVOKE_HOST_FUNCTION' }),
      heavy: heavyOf({}),
      applied: false,
      // Four nested calls, none returned — the trap is in the deepest,
      // BELOW the default depth-2 fold, so this also proves unfinished
      // branches auto-expand down to the trap point.
      diagnosticEvents: [
        call('outer', 0),
        call('mid', 1),
        call('inner', 2),
        call('trap', 3),
      ],
    });
    expect(screen.getByText('trap()')).toBeTruthy();
    expect(screen.getAllByText('stopped here')).toHaveLength(1);
  });
});
