import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';
import { render, screen } from '@testing-library/react';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';

import { HumanizedSentence, sentenceIds } from './HumanizedSentence.js';

// Real mainnet strkeys — 56 chars. The 54-char placeholders used elsewhere in
// the suite are deliberately NOT linkable: only a well-formed strkey gets a
// link, so a malformed value can never point at a page.
const DEST = 'GC4QMEH5CY5HAEZVC2XNTRV2XBPQWUX2WCV3ANU32HBFNCYIKWHGK7XQ';
const CONTRACT = 'CDDTJ7OZU2WZAEZNTUZWIRAE4EMP5CF63M3INFQWTLX4ENMYUFK6RCTX';

function light(partial: Partial<OperationItem> & { type_name: string }) {
  return {
    appearance_id: 1,
    type: 1,
    application_order: 1,
    ledger_sequence: 1,
    created_at: '2026-01-01T00:00:00Z',
    pool_ids: [],
    ...partial,
  } as OperationItem;
}

function renderSentence(
  op: OperationItem,
  heavy: XdrOperationDto | null = null
) {
  return render(
    <MemoryRouter>
      <ExplorerThemeProvider>
        <HumanizedSentence light={op} heavy={heavy} txSourceAccount={null} />
      </ExplorerThemeProvider>
    </MemoryRouter>
  );
}

describe('sentenceIds', () => {
  it('collects linkable strkeys from light and heavy details', () => {
    const ids = sentenceIds(
      light({ type_name: 'PAYMENT', destination_account: DEST }),
      {
        op_type: 'PAYMENT',
        application_order: 1,
        details: { asset: `USDC:${DEST}`, contract: CONTRACT },
        result_code: 'Success',
      } as XdrOperationDto
    );
    expect(ids.get('GC4Q…K7XQ')).toBe(DEST);
    expect(ids.get('CDDT…RCTX')).toBe(CONTRACT);
  });

  it('ignores a 56-char lookalike whose checksum does not verify', () => {
    // Shape-perfect: 56 base32 chars starting with G. Only the CRC says it is
    // not an address, and contract-supplied payloads carry such blobs.
    const tampered = `${DEST.slice(0, 10)}${
      DEST[10] === 'A' ? 'B' : 'A'
    }${DEST.slice(11)}`;
    expect(/^[GCL][A-Z2-7]{55}$/.test(tampered)).toBe(true);
    const ids = sentenceIds(
      light({ type_name: 'PAYMENT', destination_account: tampered }),
      null
    );
    expect(ids.size).toBe(0);
  });

  it('ignores ids with no detail page (hex balance ids, asset codes)', () => {
    const ids = sentenceIds(light({ type_name: 'CLAIM_CLAIMABLE_BALANCE' }), {
      op_type: 'CLAIM_CLAIMABLE_BALANCE',
      application_order: 1,
      details: { balanceId: 'ab'.repeat(32), asset: 'USDC' },
      result_code: 'Success',
    } as XdrOperationDto);
    expect(ids.size).toBe(0);
  });
});

describe('HumanizedSentence', () => {
  it('renders the identifier in the sentence as a link with a copy button', () => {
    renderSentence(light({ type_name: 'PAYMENT', destination_account: DEST }));
    // The accessible name is the FULL strkey (aria-label); the visible text
    // is the truncated form.
    const link = screen.getByRole('link', { name: DEST });
    expect(link.getAttribute('href')).toBe(`/accounts/${DEST}`);
    expect(screen.getByRole('button', { name: /copy/i })).toBeTruthy();
    // Surrounding words stay plain text, the identifier is shortened.
    expect(document.body.textContent).toContain('Sent XLM to');
    expect(document.body.textContent).toContain('GC4Q…K7XQ');
  });

  it('leaves a sentence without identifiers untouched', () => {
    renderSentence(light({ type_name: 'END_SPONSORING_FUTURE_RESERVES' }));
    expect(screen.getByText('Ended reserve sponsorship')).toBeTruthy();
    expect(screen.queryByRole('link')).toBeNull();
  });
});
