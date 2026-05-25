#!/usr/bin/env node
/**
 * Local-dev mock API for the explorer frontend.
 *
 *   node tools/dev-mock-api.mjs        # listens on http://localhost:9000
 *   PORT=9100 node tools/dev-mock-api.mjs   # custom port
 *
 * Run alongside `nx dev web`. The frontend's .env.development already
 * points VITE_API_BASE_URL at http://localhost:9000 — no env changes needed.
 *
 * Endpoints served:
 *   GET  /v1/network/stats
 *   GET  /v1/transactions/:hash            (3 fixture hashes below)
 *   GET  /v1/transactions                  (paginated list, empty cursor)
 *   GET  /v1/search?q=...                  (returns redirect for fixtures)
 *
 * Unknown hashes return 404 so the NotFoundState path is exercisable.
 */

import { createServer } from 'node:http';

const PORT = Number(process.env.PORT ?? 9000);

const networkStats = {
  generated_at: new Date().toISOString(),
  latest_ledger_closed_at: new Date(Date.now() - 6_000).toISOString(),
  latest_ledger_sequence: 54_837_201,
  total_accounts: 8_412_310,
  total_contracts: 24_891,
  transactions_per_second: 142.3,
};

const SOROBAN_HASH =
  '7b2a8c1f9d4e6a3b8c1f9d4e6a3b8c1f9d4e6a3b8c1f9d4e6a3b8c1f9d4e6a3b';
const PAYMENT_HASH =
  'a3a2c34d76a7b8c7b8b9d51c8b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9d';
const FAILED_HASH =
  'fa11edfa11edfa11edfa11edfa11edfa11edfa11edfa11edfa11edfa11ed00ff';
const MULTI_OP_HASH =
  'b07cbbbbbbbb07cb07cbbbbbbbb07cb07cbbbbbbbb07cb07cbbbbbbbb07cb07c';

const SOROBAN_TX = {
  application_order: 3,
  created_at: new Date(Date.now() - 2 * 60_000).toISOString(),
  fee_charged: 1000,
  has_soroban: true,
  hash: SOROBAN_HASH,
  ledger_sequence: 54_837_201,
  operation_count: 9,
  operations: [
    {
      appearance_id: 90_001,
      application_order: 1,
      asset_code: null,
      asset_issuer: null,
      contract_id: 'CDLLF6OCFCDC1QABMDOLR5MWOAUC4KKSPENGJG4Z3WNX5Q3DFRJU2N3J',
      created_at: new Date(Date.now() - 2 * 60_000).toISOString(),
      destination_account: null,
      ledger_sequence: 54_837_201,
      pool_id: null,
      source_account:
        'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY8H4VPF6N4WEE2N',
      type: 24,
      type_name: 'INVOKE_HOST_FUNCTION',
    },
    ...[
      'Transfer',
      'Withdrawal',
      'Deposit',
      'Transfer',
      'Withdrawal',
      'Deposit',
      'Transfer',
      'Withdrawal',
    ].map((subtype, i) => ({
      appearance_id: 90_002 + i,
      application_order: 2 + i,
      asset_code: 'USDC',
      asset_issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
      contract_id: null,
      created_at: new Date(Date.now() - (2 + i) * 60_000).toISOString(),
      destination_account: `GD2MK8J1XZN6TR2PBE7ROMNV4P3LJSTLRPAJWLJ2QVGFLBV4O0K${
        i + 1
      }`,
      ledger_sequence: 54_837_201,
      pool_id: null,
      source_account:
        'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY8H4VPF6N4WEE2N',
      type: 1,
      type_name: 'PAYMENT',
      subtype,
    })),
  ],
  parse_error: false,
  participants: [],
  soroban_events: [],
  soroban_invocations: [],
  source_account: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY8H4VPF6N4WEE2N',
  successful: true,
  heavy_fields_status: 'ok',
  heavy: {
    contract_events: [
      {
        contract_id: 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQ',
        data: { amount: 1250000000 },
        event_index: 0,
        event_type: 'contract',
        topics: ['transfer', 'GDQP...EE36', 'GAXK...7R2P'],
      },
    ],
    diagnostic_events: [
      {
        contract_id: 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQ',
        data: { success: true },
        event_index: 1,
        event_type: 'diagnostic',
        topics: ['fn_call', 'swap'],
      },
      {
        contract_id: 'CCABC1234DOLR5MWOAUC4KKSPENGJG4Z3WNX5Q3DFRJU32JF',
        data: { result: 'ok' },
        event_index: 2,
        event_type: 'diagnostic',
        topics: ['fn_return', 'transfer'],
      },
    ],
    envelope_xdr:
      'AAAAAgAAAACBYyzMyfqih3dPCEPCDeb4LIuM5l3OWHESMOh5XlcCywAAAGQB' +
      'oTfTAAAA9wAAAAEAAAAAAAAAAAAAAABl3yhSAAAAAQAAAAtTb3Jvc3dhcCB0' +
      'cmFkZQAAAAEAAAAYAAAAAAAAAAFVU0RDAAAAAEjGZIQpwh1kS3Js7T5z0qfb' +
      'AAAAAAEzfaQAAAAAAAAAAQAAAAAAAAAAAQAA',
    fee_bump_source: null,
    memo: 'Soroswap trade #54XX',
    memo_type: 'text',
    operation_tree: null,
    operations: [
      {
        application_order: 1,
        details: {
          function_name: 'swap',
          contract_label: 'Soroswap Router',
          arguments: [
            'GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOUJ3K4MOMNGEE36',
            'GAXK7R2PMNOP4RTZ',
            1_000_000_000,
            12_500_000,
          ],
          return_value: true,
          auth: {
            address: 'GDQP2KPQGKIHY...EE36',
            nonce: 42,
          },
          invocations: [
            {
              contract_id:
                'CDC1QABMDOLR5MWOAUC4KKSPENGJG4Z3WNX5Q3DFRJU32JFKLMN',
              contract_label: 'USDC Token',
              function_name: 'transferFrom',
              destination_account:
                'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY8HTR2PBCDX',
              destination_summary: 'receives 1,250 USDC',
            },
          ],
          summary_line_1: 'Swapped 100 XLM for 1,250 USDC',
          summary_line_2: 'via Soroswap · rate 1 XLM = 12.5 USDC',
        },
        op_type: 'invoke_host_function',
      },
    ],
    result_code: 'txSuccess',
    result_xdr: 'AAAAAAAAAGQAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAA=',
    results_meta_xdr:
      'AAAAAgAAAACBYyzMyfqih3dPCEPCDeb4LIuM5l3OWHESMOh5XlcCywAAAGQB' +
      'oTfTAAAA9wAAAAEAAAAAAAAAAAAAAABl3yhSAAAAAQAAAAtTb3Jvc3dhcCB0' +
      'cmFkZQAAAAEAAAAYAAAAAAAAAAFVU0RDAAAAAEjGZIQpwh1kS3Js7T5z0qfb' +
      'AAAAAAEzfaQAAAAAAAAAAQAAAAAAAAAAAQAA',
    signatures: [
      {
        hint: 'de14fb01',
        signature:
          '5dee14fb01b9c2a784df3a51e8a4b7c2d8e3f5a6b7c8d9e0f1a2b3c4d5e6f7a8' +
          '9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c',
        signer: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY8H4VPF6N4WEE2N',
        weight: 1,
      },
    ],
  },
};

const PAYMENT_TX = {
  application_order: 7,
  created_at: new Date(Date.now() - 10 * 60_000).toISOString(),
  fee_charged: 100,
  has_soroban: false,
  hash: PAYMENT_HASH,
  ledger_sequence: 54_837_198,
  operation_count: 1,
  operations: [
    {
      appearance_id: 89_500,
      application_order: 1,
      asset_code: 'USDC',
      asset_issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
      contract_id: null,
      created_at: new Date(Date.now() - 10 * 60_000).toISOString(),
      destination_account:
        'GD2MK8J1XZN6TR2PBE7ROMNV4P3LJSTLRPAJWLJ2QVGFLBV4OOKMK8J1',
      ledger_sequence: 54_837_198,
      pool_id: null,
      source_account:
        'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY8H4VPF6N4WEE2N',
      type: 1,
      type_name: 'PAYMENT',
    },
  ],
  parse_error: false,
  participants: [],
  soroban_events: [],
  soroban_invocations: [],
  source_account: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY8H4VPF6N4WEE2N',
  successful: true,
  heavy_fields_status: 'ok',
  heavy: {
    contract_events: [],
    diagnostic_events: [],
    envelope_xdr:
      'AAAAAgAAAACBYyzMyfqih3dPCEPCDeb4LIuM5l3OWHESMOh5XlcCywAAAGQA' +
      'oTfTAAAAEgAAAAEAAAAAAAAAAA==',
    fee_bump_source: null,
    memo: null,
    memo_type: 'none',
    operation_tree: null,
    operations: [
      {
        application_order: 1,
        details: {
          asset: { code: 'USDC', issuer: 'GA5Z…KZVN' },
          amount: '1000000000',
        },
        op_type: 'payment',
      },
    ],
    result_code: 'txSuccess',
    result_xdr: 'AAAAAAAAAGQAAAAAAAAAAQAAAAA=',
    signatures: [
      {
        hint: 'a1b2c3d4',
        signature: '0123456789abcdef'.repeat(8),
        signer: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY8H4VPF6N4WEE2N',
        weight: 2,
      },
    ],
  },
};

const FAILED_TX = {
  ...PAYMENT_TX,
  hash: FAILED_HASH,
  successful: false,
  application_order: 4,
  heavy: {
    ...PAYMENT_TX.heavy,
    result_code: 'txFailed',
    memo: null,
    memo_type: 'none',
  },
};

function multiOp(index, destSuffix) {
  return {
    appearance_id: 92_000 + index,
    application_order: index,
    asset_code: 'USDC',
    asset_issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
    contract_id: null,
    created_at: new Date(Date.now() - 30 * 60_000).toISOString(),
    destination_account: `GD2MK8J1XZN6TR2PBE7ROMNV4P3LJSTLRPAJWLJ2QVGFLBV4O${destSuffix}`,
    ledger_sequence: 54_837_100,
    pool_id: null,
    source_account: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY8H4VPF6N4WEE2N',
    type: 1,
    type_name: 'PAYMENT',
  };
}

const MULTI_OP_TX = {
  application_order: 11,
  created_at: new Date(Date.now() - 30 * 60_000).toISOString(),
  fee_charged: 500,
  has_soroban: false,
  hash: MULTI_OP_HASH,
  ledger_sequence: 54_837_100,
  operation_count: 5,
  operations: [
    multiOp(1, 'XKMK1'),
    multiOp(2, 'XKMK2'),
    multiOp(3, 'XKMK3'),
    multiOp(4, 'XKMK4'),
    multiOp(5, 'XKMK5'),
  ],
  parse_error: false,
  participants: [],
  soroban_events: [],
  soroban_invocations: [],
  source_account: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY8H4VPF6N4WEE2N',
  successful: true,
  heavy_fields_status: 'ok',
  heavy: {
    contract_events: [],
    diagnostic_events: [],
    envelope_xdr: 'AAAAAgAAAA...mockMultiOpEnvelope==',
    fee_bump_source: null,
    memo: 'Batch payout',
    memo_type: 'text',
    operation_tree: null,
    operations: Array.from({ length: 5 }, (_unused, i) => ({
      application_order: i + 1,
      details: {
        asset: { code: 'USDC', issuer: 'GA5Z…KZVN' },
        amount: String(100_000_000 * (i + 1)),
      },
      op_type: 'payment',
    })),
    result_code: 'txSuccess',
    result_xdr: 'AAAAAAAAAGQAAAAAAAAAAQAAAAAAAAA=',
    signatures: [
      {
        hint: 'c0ffee01',
        signature: 'beefcafedeadbabe'.repeat(8),
        signer: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY8H4VPF6N4WEE2N',
        weight: 1,
      },
    ],
  },
};

const TX_BY_HASH = {
  [SOROBAN_HASH]: SOROBAN_TX,
  [PAYMENT_HASH]: PAYMENT_TX,
  [FAILED_HASH]: FAILED_TX,
  [MULTI_OP_HASH]: MULTI_OP_TX,
};

const LIST_RESPONSE = {
  data: [
    {
      hash: SOROBAN_HASH,
      ledger_sequence: SOROBAN_TX.ledger_sequence,
      source_account: SOROBAN_TX.source_account,
      successful: true,
      fee_charged: SOROBAN_TX.fee_charged,
      created_at: SOROBAN_TX.created_at,
      operation_types: ['INVOKE_HOST_FUNCTION'],
    },
    {
      hash: PAYMENT_HASH,
      ledger_sequence: PAYMENT_TX.ledger_sequence,
      source_account: PAYMENT_TX.source_account,
      successful: true,
      fee_charged: PAYMENT_TX.fee_charged,
      created_at: PAYMENT_TX.created_at,
      operation_types: ['PAYMENT'],
    },
    {
      hash: FAILED_HASH,
      ledger_sequence: FAILED_TX.ledger_sequence,
      source_account: FAILED_TX.source_account,
      successful: false,
      fee_charged: FAILED_TX.fee_charged,
      created_at: FAILED_TX.created_at,
      operation_types: ['PAYMENT'],
    },
    {
      hash: MULTI_OP_HASH,
      ledger_sequence: MULTI_OP_TX.ledger_sequence,
      source_account: MULTI_OP_TX.source_account,
      successful: true,
      fee_charged: MULTI_OP_TX.fee_charged,
      created_at: MULTI_OP_TX.created_at,
      operation_types: ['PAYMENT'],
    },
  ],
  page: { cursor: null, has_more: false, limit: 20 },
};

const CORS_HEADERS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'GET,OPTIONS',
  'Access-Control-Allow-Headers': 'Content-Type,Authorization',
};

function send(res, status, body) {
  res.writeHead(status, {
    'Content-Type': 'application/json',
    'Cache-Control': 'no-store',
    ...CORS_HEADERS,
  });
  res.end(JSON.stringify(body));
}

function notFound(res, code = 'not_found', message = 'Not found') {
  send(res, 404, { code, message });
}

const server = createServer((req, res) => {
  if (req.method === 'OPTIONS') {
    res.writeHead(204, CORS_HEADERS);
    res.end();
    return;
  }
  const url = new URL(req.url ?? '/', `http://${req.headers.host}`);
  const path = url.pathname;

  if (path === '/v1/network/stats') {
    send(res, 200, {
      ...networkStats,
      generated_at: new Date().toISOString(),
    });
    return;
  }

  if (path === '/v1/transactions') {
    send(res, 200, LIST_RESPONSE);
    return;
  }

  const txMatch = /^\/v1\/transactions\/([0-9a-f]{64})$/.exec(path);
  if (txMatch) {
    const tx = TX_BY_HASH[txMatch[1]];
    if (tx == null) {
      notFound(res, 'transaction_not_found', 'Transaction not indexed');
      return;
    }
    send(res, 200, tx);
    return;
  }

  if (path === '/v1/search') {
    const q = (url.searchParams.get('q') ?? '').toLowerCase();
    if (TX_BY_HASH[q] != null) {
      send(res, 200, {
        entity_type: 'transaction',
        canonical: q,
        target: { path: `/transactions/${q}` },
      });
      return;
    }
    send(res, 200, {
      results: [],
      total_count: 0,
      page: { cursor: null, has_more: false, limit: 20 },
    });
    return;
  }

  notFound(res, 'route_not_found', `No mock for ${req.method} ${path}`);
});

server.listen(PORT, () => {
  // eslint-disable-next-line no-console
  console.log(`[dev-mock-api] listening on http://localhost:${PORT}`);
  console.log('  fixture transaction hashes:');
  console.log(`    Soroban swap : ${SOROBAN_HASH}`);
  console.log(`    Payment      : ${PAYMENT_HASH}`);
  console.log(`    Failed       : ${FAILED_HASH}`);
  console.log(`    Multi-op     : ${MULTI_OP_HASH}`);
});
