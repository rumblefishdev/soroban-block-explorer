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
  tps_60s: 142.3,
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
        'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N',
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
        'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N',
      type: 1,
      type_name: 'PAYMENT',
      subtype,
    })),
  ],
  parse_error: false,
  participants: [],
  soroban_events: [],
  soroban_invocations: [],
  source_account: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N',
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
        signer: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N',
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
        'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N',
      type: 1,
      type_name: 'PAYMENT',
    },
  ],
  parse_error: false,
  participants: [],
  soroban_events: [],
  soroban_invocations: [],
  source_account: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N',
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
        signer: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N',
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
    source_account: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N',
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
  source_account: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N',
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
        signer: 'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N',
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

// ────────────────────────────────────────────────────────────────────
// Asset fixtures (Figma node 188:25087 list + 206:15683 detail).
// `id` is the surrogate; the frontend may also look up an asset via the
// `code-issuer` composite or the contract C-strkey for SAC/Soroban.
// Strkeys are CAP-23 base32 ([A-Z2-7]) so `isContractId` / `isAccountId`
// accept them. asset_type: 0 native, 1 classic_credit, 2 sac, 3 soroban.
const ASSET_FIXTURES = [
  {
    id: 1,
    asset_code: 'USDC',
    asset_type: 1,
    asset_type_name: 'classic_credit',
    issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
    contract_id: null,
    holder_count: 42_891,
    icon_url: null,
    name: 'USD Coin',
    total_supply: '1234567890.0000000',
    description:
      'USD Coin (USDC) is a digital dollar backed 100% by highly liquid cash and cash-equivalent assets.',
    home_page: 'https://www.circle.com/usdc',
  },
  {
    id: 2,
    asset_code: 'USDC',
    asset_type: 2,
    asset_type_name: 'sac',
    issuer: null,
    contract_id: 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC',
    holder_count: 18_204,
    icon_url: null,
    name: 'USD Coin (Soroban)',
    total_supply: '892340100.0000000',
    description: 'Soroban Asset Contract wrapper for USDC.',
    home_page: 'https://www.circle.com/usdc',
    deployed_at_ledger: 53_900_000,
  },
  {
    id: 3,
    asset_code: 'EURC',
    asset_type: 1,
    asset_type_name: 'classic_credit',
    issuer: 'GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPPL',
    contract_id: null,
    holder_count: 12_440,
    icon_url: null,
    name: 'Euro Coin',
    total_supply: '421890000.0000000',
    description: 'Euro-backed stablecoin issued by Circle.',
    home_page: 'https://www.circle.com/eurc',
  },
  {
    id: 4,
    asset_code: 'SWAPY',
    asset_type: 3,
    asset_type_name: 'soroban',
    issuer: null,
    contract_id: 'CCABCDEFGHIJKLMN23OPQRSTUVWXYZ34567ABCDEFGHIJKLMNOPQR5678',
    holder_count: 8_912,
    icon_url: null,
    name: 'Soroswap Token',
    total_supply: '100000000.0000000',
    description: 'Native governance token for the Soroswap protocol.',
    home_page: 'https://soroswap.finance',
    deployed_at_ledger: 54_100_000,
  },
  {
    id: 5,
    asset_code: 'XRP',
    asset_type: 1,
    asset_type_name: 'classic_credit',
    issuer: 'GCNSGHUCG5VMGLT5RIYYZSO7VCU4ABCDEFGHIJKLMN3456PQRSTUBKJL',
    contract_id: null,
    holder_count: 6_201,
    icon_url: null,
    name: 'Ripple',
    total_supply: '98700000.0000000',
    description: 'XRP wrapped onto Stellar.',
    home_page: 'https://ripple.com',
  },
  {
    id: 6,
    asset_code: 'EURC',
    asset_type: 2,
    asset_type_name: 'sac',
    issuer: null,
    contract_id: 'CDEF234567GHIJKLMNO234PQRSTUVWXYZ234567ABCDEFGHIJKLMN9012'
      .replace(/[019]/g, '5')
      .slice(0, 56),
    holder_count: 4_890,
    icon_url: null,
    name: 'Euro Coin (Soroban)',
    total_supply: '80440000.0000000',
    description: 'SAC wrapper for EURC.',
    home_page: 'https://www.circle.com/eurc',
    deployed_at_ledger: 54_050_000,
  },
  {
    id: 7,
    asset_code: 'AQUA',
    asset_type: 3,
    asset_type_name: 'soroban',
    issuer: null,
    contract_id: 'CCGHIJKLMN234OPQRSTUVWXYZ234567ABCDEFGHIJKLMN234567PQR3456',
    holder_count: 3_201,
    icon_url: null,
    name: 'Aquarius',
    total_supply: '50000000000.0000000',
    description: 'Liquidity-mining governance token native to Aquarius.',
    home_page: 'https://aqua.network',
    deployed_at_ledger: 54_200_000,
  },
  {
    id: 8,
    asset_code: 'yXLM',
    asset_type: 1,
    asset_type_name: 'classic_credit',
    issuer: 'GARDNV3Q7YGT4AKSDF2GRGRCIEINEABCDEFGHIJKLMN23456PQRSTYLDX',
    contract_id: null,
    holder_count: 2_840,
    icon_url: null,
    name: 'yXLM by Yield',
    total_supply: '24500000.0000000',
    description: 'Yield-bearing wrapped XLM.',
    home_page: 'https://yieldlabs.io',
  },
  {
    id: 9,
    asset_code: 'BLND',
    asset_type: 3,
    asset_type_name: 'soroban',
    issuer: null,
    contract_id: 'CCJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMN234567ABCDEFGHIJ7890'
      .replace(/[019]/g, '5')
      .slice(0, 56),
    holder_count: 1_920,
    icon_url: null,
    name: 'Blend Protocol',
    total_supply: '10000000.0000000',
    description: 'Money-market protocol on Soroban.',
    home_page: 'https://blend.capital',
    deployed_at_ledger: 54_250_000,
  },
  {
    id: 10,
    asset_code: 'BTC',
    asset_type: 1,
    asset_type_name: 'classic_credit',
    issuer: 'GAUTUYY2THLF7SGITDFMXJVYH3IMABCDEFGHIJKLMN234567PQRSTNKQV',
    contract_id: null,
    holder_count: 1_204,
    icon_url: null,
    name: 'Bitcoin',
    total_supply: '8420000.0000000',
    description: 'Wrapped BTC on Stellar.',
    home_page: 'https://bitcoin.org',
  },
];

// Soroban token-like interface used for the primary contract fixture —
// mirrors the Figma Interface tab (transfer / balance / approve / allowance
// / mint). Shape matches `parseInterfaceMetadata` in the frontend.
const TOKEN_INTERFACE = {
  functions: [
    {
      name: 'transfer',
      doc: '',
      inputs: [
        { name: 'from', type_name: 'Address' },
        { name: 'to', type_name: 'Address' },
        { name: 'amount', type_name: 'i128' },
      ],
      outputs: ['bool'],
    },
    {
      name: 'balance',
      doc: '',
      inputs: [{ name: 'id', type_name: 'Address' }],
      outputs: ['i128'],
    },
    {
      name: 'approve',
      doc: '',
      inputs: [
        { name: 'from', type_name: 'Address' },
        { name: 'spender', type_name: 'Address' },
        { name: 'amount', type_name: 'i128' },
        { name: 'expiration_ledger', type_name: 'u32' },
      ],
      outputs: [],
    },
    {
      name: 'allowance',
      doc: '',
      inputs: [
        { name: 'from', type_name: 'Address' },
        { name: 'spender', type_name: 'Address' },
      ],
      outputs: ['i128'],
    },
    {
      name: 'mint',
      doc: '',
      inputs: [
        { name: 'to', type_name: 'Address' },
        { name: 'amount', type_name: 'i128' },
      ],
      outputs: [],
    },
  ],
  wasm_byte_len: 23_412,
};

const CONTRACT_FIXTURES = [
  {
    contract_id: 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC',
    contract_type: 3,
    contract_type_name: 'soroban',
    deployed_at_ledger: 48_200_100,
    // Matches the Figma "GDQP...EE36" breadcrumb deployer.
    deployer: 'GDQP3NX42SDCQGCWN7ZTQF7BS4FRTHX2YH6IBM3QDH2XYV5HJUYZEE36',
    is_sac: false,
    stats: {
      recent_invocations: 1_284,
      recent_unique_callers: 342,
      stats_window: '7 days',
    },
    wasm_hash:
      'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2',
    wasm_uploaded_at_ledger: 48_200_098,
    interface: TOKEN_INTERFACE,
  },
  {
    contract_id: 'CCABCDEFGHIJKLMN23OPQRSTUVWXYZ34567ABCDEFGHIJKLMNOPQR5678',
    contract_type: 2,
    contract_type_name: 'sac',
    deployed_at_ledger: 50_100_500,
    deployer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
    is_sac: true,
    stats: {
      recent_invocations: 412,
      recent_unique_callers: 87,
      stats_window: '7 days',
    },
    wasm_hash: null,
    wasm_uploaded_at_ledger: null,
    interface: null,
  },
];

function findContract(id) {
  return CONTRACT_FIXTURES.find((c) => c.contract_id === id) ?? null;
}

// Cycles function names so the invocations table shows varied entries.
const CONTRACT_FN_POOL = [
  'swap',
  'transfer',
  'approve',
  'balance',
  'mint',
  'allowance',
];

function buildContractInvocationRow(i, contractId) {
  const fn = CONTRACT_FN_POOL[i % CONTRACT_FN_POOL.length];
  // Reuse the synthetic tx pool so each invocation row links to a real-ish
  // transaction hash in the global tx list.
  const tx = buildSyntheticTxRow(i + 4);
  return {
    amount: 1,
    caller_account: ACCOUNT_POOL[i % ACCOUNT_POOL.length],
    created_at: tx.created_at,
    ledger_sequence: tx.ledger_sequence,
    successful: i % 6 !== 5,
    transaction_hash: tx.hash,
    // Frontend ignores extra fields — keep `fn` so we can verify the mock
    // by inspecting the network response.
    function_name: fn,
    contract_id: contractId,
  };
}

const EVENT_KIND_POOL = ['contract', 'system', 'diagnostic'];
const EVENT_TOPIC_POOL = [
  [
    'transfer',
    'GDQP3NX42SDCQGCWN7ZTQF7BS4FRTHX2YH6IBM3QDH2XYV5HJUYZEE36',
    'GAXK7BCQEZN6Y3SBYPRTKDS5ENRR2XFMC5MLI2P3JK3FRTPXDR4S7R2P',
  ],
  ['fn_call', 'swap'],
  ['approve', 'GCZT2JS37VBOM6CSEQENVQI4VBKVK5NWBCRTHA52IIBGZZQNI5DTM7KL'],
  ['fn_return', 'transfer'],
];

function buildContractEventRow(i, contractId) {
  const tx = buildSyntheticTxRow(i + 4);
  const topics = EVENT_TOPIC_POOL[i % EVENT_TOPIC_POOL.length];
  return {
    amount: 1,
    created_at: tx.created_at,
    data: ACCOUNT_POOL[i % ACCOUNT_POOL.length],
    event_type: EVENT_KIND_POOL[i % EVENT_KIND_POOL.length],
    ledger_sequence: tx.ledger_sequence,
    successful: true,
    topics,
    transaction_hash: tx.hash,
    transaction_id: 9_000_000 + i,
    contract_id: contractId,
  };
}

// NFT collections — `contract_id` is the issuing C-strkey, `name` is the
// human label shown in the Collection column. Tokens reference these by
// `contract_id`.
function findAsset(id) {
  // Accept the surrogate (numeric or numeric-as-string), the
  // `code-issuer` composite, or the contract C-strkey.
  if (/^\d+$/.test(id)) {
    const n = Number(id);
    return ASSET_FIXTURES.find((a) => a.id === n) ?? null;
  }
  if (id.startsWith('C')) {
    return ASSET_FIXTURES.find((a) => a.contract_id === id) ?? null;
  }
  if (id.includes('-')) {
    const [code, issuer] = id.split('-');
    return (
      ASSET_FIXTURES.find(
        (a) => a.asset_code === code && a.issuer === issuer
      ) ?? null
    );
  }
  return null;
}

// Drives the Figma "empty" account fixture (node 157:22228). Visit
//   /accounts/GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB6PFY
// to render zero balances + zero transactions. Valid CAP-23 base32.
const EMPTY_ACCOUNT_ID =
  'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB6PFY';

// Pool of varied accounts for synthetic list rows — keeps the home tables
// visually diverse without needing distinct detail-page fixtures for each.
// Each value is a valid CAP-23 base32 strkey (G + 55 chars from [A-Z2-7])
// so the frontend `isAccountId` validator accepts them and the
// `/accounts/:id` page loads.
const ACCOUNT_POOL = [
  'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N',
  'GDQP3NX42SDCQGCWN7ZTQF7BS4FRTHX2YH6IBM3QDH2XYV5HJUYZEE36',
  'GAXK7BCQEZN6Y3SBYPRTKDS5ENRR2XFMC5MLI2P3JK3FRTPXDR4S7R2P',
  'GCZT2JS37VBOM6CSEQENVQI4VBKVK5NWBCRTHA52IIBGZZQNI5DTM7KL',
  'GFRTQRBC36PCT4ZHN5HOAR4D2GJASRINOOAEVOZJZSWTOK4XEY7G6KPQ',
  'GHVBPKW7VLD4WMSP4OFTQ2FGBKWC5UMHO27YPV5UPN5DZJ7M2YMNK2MN',
  'GMNPSAIBRQHHHKKWO64ZEXJ7BCYS5QNVNHMG43OOZWCK6F2GLAHX4RTZ',
  'GKWXM6QYTZTAVZJKGDPM4K2RAFRZNNGXEACVRWQHEHSBCNB2DTKE7PLM',
  'GRSTMKHHTGCKHEAQLRQI3WGRSTZQ3DCWPNS66LQRTNXJN5ZHBNMR4UVW',
  'GYZW3UEZK5BG6BIBQCTEKWNL4TLW3HUWWDFUQWMKNFTSXYBV2HE65QRS',
  'GABCJYDLDF5IBNVZRGY3KK2I4Q6F6OWMC4KIEZQRWHCATFZKE2RJ7XYZ',
];

const OP_TYPE_POOL = [
  ['PAYMENT'],
  ['INVOKE_HOST_FUNCTION'],
  ['PATH_PAYMENT_STRICT_RECEIVE'],
  ['CHANGE_TRUST'],
  ['MANAGE_BUY_OFFER'],
  ['LIQUIDITY_POOL_DEPOSIT'],
  ['CREATE_ACCOUNT'],
  ['MANAGE_SELL_OFFER'],
];

function buildSyntheticTxRow(i) {
  const prefix = (i * 7919).toString(16).padStart(6, '0').slice(0, 6);
  const suffix = (i * 6151 + 1).toString(16).padStart(4, '0').slice(0, 4);
  const hash = `${prefix}${'a'.repeat(54)}${suffix}`;
  return {
    hash,
    ledger_sequence: 54_837_201 - i,
    source_account: ACCOUNT_POOL[i % ACCOUNT_POOL.length],
    successful: i % 5 !== 3, // every 5th row fails — keeps the table varied
    fee_charged: ((100 + (i % 7) * 50) * 10).toString(),
    created_at: new Date(Date.now() - (i + 1) * 6 * 60_000).toISOString(),
    operation_types: OP_TYPE_POOL[i % OP_TYPE_POOL.length],
  };
}

const NFT_COLLECTIONS = [
  {
    contract_id: 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC',
    name: 'Stellar Punks',
  },
  {
    contract_id: 'CCABCDEFGHIJKLMN23OPQRSTUVWXYZ34567ABCDEFGHIJKLMNOPQR5678',
    name: 'Galaxy Series',
  },
  {
    contract_id: 'CCGHIJKLMN234OPQRSTUVWXYZ234567ABCDEFGHIJKLMN234567PQR3456',
    name: 'Stellar Apes',
  },
  {
    contract_id: 'CCJKLMN234OPQRSTUVWXYZ234567ABCDEFGHIJKLMN234567ABCDEF7890',
    name: 'Sorostars',
  },
  {
    contract_id: 'CDDEF234567GHIJKLMN234PQRSTUVWXYZ234567ABCDEFGHIJKLMN9012',
    name: 'Pixel Stellar',
  },
  {
    contract_id: 'CDABCD2345GHIJKLMN23OPQRSTUVWXYZ34567ABCDEFGHIJKLMN3456PQ',
    name: 'Stellar Bloom',
  },
  {
    contract_id: 'CDQRS234567TUVWXYZ234ABCDEFGHIJKLMN234567OPQRSTUVWXYZ7890',
    name: 'Deep Space',
  },
  {
    contract_id: 'CDXYZ23456ABCDEFGHIJKLMN23OPQRSTUVWXYZ34567ABCDEFGHI4567L',
    name: 'Lumens Art',
  },
];

// Predictable trait set used across the mock — Figma frame 261:13916
// shows 6 traits ("Background, Eyes, Headwear, Skin, Mouth, Accessory")
// each with a "X% have this" sub-line. Frontend reads `rarity_percent`.
const NFT_TRAITS_TEMPLATE = [
  { trait_type: 'Background', value: 'Cosmic Blue', rarity_percent: 12 },
  { trait_type: 'Eyes', value: 'Laser Red', rarity_percent: 8 },
  { trait_type: 'Headwear', value: 'Gold Crown', rarity_percent: 3 },
  { trait_type: 'Skin', value: 'Alien Green', rarity_percent: 5 },
  { trait_type: 'Mouth', value: 'Smile', rarity_percent: 24 },
  { trait_type: 'Accessory', value: 'None', rarity_percent: 41 },
];

function buildNftRow(i) {
  const collection = NFT_COLLECTIONS[i % NFT_COLLECTIONS.length];
  const tokenNum = 1042 + ((i * 17) % 900);
  // Three media states cycle: loaded image (picsum), no image (null),
  // broken url. Matches the Figma list which shows all three.
  const mediaCycle = i % 7;
  let media_url = null;
  if (mediaCycle < 5) {
    media_url = `https://picsum.photos/seed/punk${tokenNum}/200`;
  } else if (mediaCycle === 6) {
    media_url = 'https://invalid.example/missing.png';
  }
  return {
    id: 10_000 + i,
    contract_id: collection.contract_id,
    collection_name: collection.name,
    name: `${collection.name} #${tokenNum}`,
    token_id: String(tokenNum),
    media_url,
    owner_account: ACCOUNT_POOL[i % ACCOUNT_POOL.length],
    minted_at_ledger: 52_100_400 - i * 17,
    last_seen_ledger: 54_821_301 - i * 5,
  };
}

const NFT_FIXTURES = Array.from({ length: 60 }, (_, i) => buildNftRow(i));

function findNftToken(contractId, tokenId) {
  return (
    NFT_FIXTURES.find(
      (n) => n.contract_id === contractId && n.token_id === tokenId
    ) ?? null
  );
}

const NFT_TRANSFER_EVENTS = ['mint', 'transfer', 'transfer', 'transfer'];

function buildNftTransferRow(i, contractId, tokenId) {
  const tx = buildSyntheticTxRow(i + 4);
  const event = NFT_TRANSFER_EVENTS[i % NFT_TRANSFER_EVENTS.length];
  const isMint = event === 'mint';
  return {
    contract_id: contractId,
    token_id: tokenId,
    created_at: tx.created_at,
    event_order: i,
    // Raw discriminant (ADR 0031): 0=mint, 1=transfer, 2=burn.
    event_type: isMint ? 0 : 1,
    event_type_name: event,
    from_account: isMint ? null : ACCOUNT_POOL[(i + 1) % ACCOUNT_POOL.length],
    to_account: ACCOUNT_POOL[i % ACCOUNT_POOL.length],
    transaction_hash: tx.hash,
    ledger_sequence: tx.ledger_sequence,
  };
}

// Helper: build a `PoolAssetLeg` object matching the API shape. Native XLM
// uses asset_type=0, classic credit=1, SAC=2.
function nativeLeg() {
  return {
    asset_code: 'XLM',
    asset_type: 0,
    asset_type_name: 'native',
    contract_id: null,
    issuer: null,
  };
}
function classicLeg(code, issuer, sacContractId = null) {
  return {
    asset_code: code,
    asset_type: 1,
    asset_type_name: 'classic_credit',
    contract_id: sacContractId,
    issuer,
  };
}

// 8 pools matching the Figma list. SEP-23 pool IDs start with `L` and are
// 56 chars from the CAP-23 base32 alphabet. Synthetic strkeys are not
// cryptographically valid but pass `isPoolId` (regex check only).
const POOL_FIXTURES = [
  {
    pool_id: 'LBJRKPHNDQX6FMT2BIPW5ELSZAHOV4DKRY7GNU3CJQX6FMT2BIPW5ELS',
    asset_a: nativeLeg(),
    asset_b: classicLeg(
      'USDC',
      'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
      'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC'
    ),
    fee_bps: 30,
    fee_percent: '0.30',
    reserve_a: '1200000.0000000',
    reserve_b: '480000.0000000',
    total_shares: '753982100.0000000',
    tvl: '2400000',
    volume: '85000',
    fee_revenue: '255',
    participant_count: 1284,
    created_at_ledger: 48_200_100,
    latest_snapshot_at: new Date(Date.now() - 5 * 60_000).toISOString(),
    latest_snapshot_ledger: 54_837_201,
  },
  {
    pool_id: 'LEMU2RVNEQX6FMT2BIPW5ELSZAHOV4DKRY7GNU3CJQX6FMT2BIPW5ELS',
    asset_a: classicLeg(
      'USDC',
      'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN'
    ),
    asset_b: classicLeg(
      'EURC',
      'GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPPL'
    ),
    fee_bps: 30,
    fee_percent: '0.30',
    reserve_a: '980000.0000000',
    reserve_b: '392000.0000000',
    total_shares: '620441800.0000000',
    tvl: '1900000',
    volume: '62000',
    fee_revenue: '186',
    participant_count: 892,
    created_at_ledger: 49_000_500,
    latest_snapshot_at: new Date(Date.now() - 5 * 60_000).toISOString(),
    latest_snapshot_ledger: 54_837_200,
  },
  {
    pool_id: 'LHPXWNYLFQX6FMT2BIPW5ELSZAHOV4DKRY7GNU3CJQX6FMT2BIPW5ELS',
    asset_a: nativeLeg(),
    asset_b: classicLeg(
      'AQUA',
      'GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA'
    ),
    fee_bps: 30,
    fee_percent: '0.30',
    reserve_a: '840000.0000000',
    reserve_b: '2100000000.0000000',
    total_shares: '421200400.0000000',
    tvl: '1680000',
    volume: '40000',
    fee_revenue: '120',
    participant_count: 641,
    created_at_ledger: 50_100_700,
    latest_snapshot_at: new Date(Date.now() - 5 * 60_000).toISOString(),
    latest_snapshot_ledger: 54_837_199,
  },
  {
    pool_id: 'LKS2XQG4GQX6FMT2BIPW5ELSZAHOV4DKRY7GNU3CJQX6FMT2BIPW5ELS',
    asset_a: nativeLeg(),
    asset_b: classicLeg(
      'BTC',
      'GAUTUYY2THLF7SGITDFMXJVYH3IMABCDEFGHIJKLMN234567PQRSTNKQV'
    ),
    fee_bps: 30,
    fee_percent: '0.30',
    reserve_a: '560000.0000000',
    reserve_b: '142.0000000',
    total_shares: '284100200.0000000',
    tvl: '1120000',
    volume: '38000',
    fee_revenue: '114',
    participant_count: 412,
    created_at_ledger: 51_200_900,
    latest_snapshot_at: new Date(Date.now() - 5 * 60_000).toISOString(),
    latest_snapshot_ledger: 54_837_198,
  },
  {
    pool_id: 'LNV5ITTDHQX6FMT2BIPW5ELSZAHOV4DKRY7GNU3CJQX6FMT2BIPW5ELS',
    asset_a: classicLeg(
      'USDC',
      'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN'
    ),
    asset_b: classicLeg(
      'SWAPY',
      'GBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N'
    ),
    fee_bps: 30,
    fee_percent: '0.30',
    reserve_a: '320000.0000000',
    reserve_b: '8400000.0000000',
    total_shares: '198400100.0000000',
    tvl: '640000',
    volume: '22000',
    fee_revenue: '66',
    participant_count: 284,
    created_at_ledger: 52_100_400,
    latest_snapshot_at: new Date(Date.now() - 5 * 60_000).toISOString(),
    latest_snapshot_ledger: 54_837_197,
  },
  {
    pool_id: 'LQYA2RV4IQX6FMT2BIPW5ELSZAHOV4DKRY7GNU3CJQX6FMT2BIPW5ELS',
    asset_a: nativeLeg(),
    asset_b: classicLeg(
      'EURC',
      'GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPPL'
    ),
    fee_bps: 30,
    fee_percent: '0.30',
    reserve_a: '410000.0000000',
    reserve_b: '164000.0000000',
    total_shares: '142800900.0000000',
    tvl: '820000',
    volume: '18000',
    fee_revenue: '54',
    participant_count: 198,
    created_at_ledger: 52_500_100,
    latest_snapshot_at: new Date(Date.now() - 5 * 60_000).toISOString(),
    latest_snapshot_ledger: 54_837_196,
  },
  {
    pool_id: 'LT3DXIH4JQX6FMT2BIPW5ELSZAHOV4DKRY7GNU3CJQX6FMT2BIPW5ELS',
    asset_a: nativeLeg(),
    asset_b: classicLeg(
      'BLND',
      'GAXK7BCQEZN6Y3SBYPRTKDS5ENRR2XFMC5MLI2P3JK3FRTPXDR4S7R2P'
    ),
    fee_bps: 30,
    fee_percent: '0.30',
    reserve_a: '280000.0000000',
    reserve_b: '1400000.0000000',
    total_shares: '98200400.0000000',
    tvl: '560000',
    volume: '12000',
    fee_revenue: '36',
    participant_count: 142,
    created_at_ledger: 53_100_200,
    latest_snapshot_at: new Date(Date.now() - 5 * 60_000).toISOString(),
    latest_snapshot_ledger: 54_837_195,
  },
  {
    pool_id: 'LW6GHMV4KQX6FMT2BIPW5ELSZAHOV4DKRY7GNU3CJQX6FMT2BIPW5ELS',
    asset_a: classicLeg(
      'USDC',
      'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN'
    ),
    asset_b: classicLeg(
      'XRP',
      'GCZT2JS37VBOM6CSEQENVQI4VBKVK5NWBCRTHA52IIBGZZQNI5DTM7KL'
    ),
    fee_bps: 30,
    fee_percent: '0.30',
    reserve_a: '190000.0000000',
    reserve_b: '380000.0000000',
    total_shares: '72400100.0000000',
    tvl: '380000',
    volume: '8000',
    fee_revenue: '24',
    participant_count: 98,
    created_at_ledger: 53_500_900,
    latest_snapshot_at: new Date(Date.now() - 5 * 60_000).toISOString(),
    latest_snapshot_ledger: 54_837_194,
  },
];

function findPool(id) {
  return POOL_FIXTURES.find((p) => p.pool_id === id) ?? null;
}

function buildPoolParticipantRow(i, pool) {
  return {
    account: ACCOUNT_POOL[i % ACCOUNT_POOL.length],
    shares: ((Number(pool.total_shares) * (11 - i)) / 90).toFixed(7),
    share_percentage: ((11 - i) * 1.13).toFixed(2),
    first_deposit_ledger: pool.created_at_ledger + i * 100_000,
    last_updated_ledger: 54_821_301 - i * 5,
  };
}

const POOL_TX_OP_TYPES = [
  ['PATH_PAYMENT_STRICT_SEND'],
  ['LIQUIDITY_POOL_DEPOSIT'],
  ['PATH_PAYMENT_STRICT_RECEIVE'],
  ['LIQUIDITY_POOL_WITHDRAW'],
  ['PATH_PAYMENT_STRICT_SEND'],
  ['LIQUIDITY_POOL_DEPOSIT'],
];

function buildPoolTransactionRow(i) {
  const tx = buildSyntheticTxRow(i + 4);
  return {
    created_at: tx.created_at,
    fee_charged: tx.fee_charged,
    has_soroban: false,
    hash: tx.hash,
    ledger_sequence: tx.ledger_sequence,
    operation_count: 1,
    operation_types: POOL_TX_OP_TYPES[i % POOL_TX_OP_TYPES.length],
    source_account: ACCOUNT_POOL[i % ACCOUNT_POOL.length],
    successful: true,
  };
}

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
    ...Array.from({ length: 96 }, (_, i) => buildSyntheticTxRow(i + 4)),
  ],
  page: { limit: 100 },
};

function buildLedgerRow(i) {
  const prefix = (i * 9973 + 0x10000).toString(16).slice(0, 6);
  const hash = `${prefix}${'b'.repeat(58)}`.slice(0, 64);
  const txCountCycle = [14, 8, 21, 0, 14, 8, 21, 0, 6, 12, 3, 17, 22, 9, 11];
  return {
    sequence: 54_837_201 - i,
    hash,
    closed_at: new Date(Date.now() - (i * 6_000 + 4_000)).toISOString(),
    protocol_version: 21,
    transaction_count: txCountCycle[i % txCountCycle.length],
    base_fee: 100,
  };
}

const LEDGER_LIST_RESPONSE = {
  data: Array.from({ length: 100 }, (_, i) => buildLedgerRow(i)),
  page: { limit: 100 },
};

const CORS_HEADERS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'GET,OPTIONS',
  'Access-Control-Allow-Headers': 'Content-Type,Authorization',
};

function clampLimit(raw) {
  const n = Number.parseInt(raw ?? '', 10);
  if (!Number.isFinite(n) || n <= 0) return 20;
  return Math.min(n, 100);
}

/** Cursor = base-10 offset into the pool. Opaque to the client but
 *  trivially parsable here so the mock can slice deterministically. */
function parseCursor(raw) {
  if (raw == null) return 0;
  const n = Number.parseInt(raw, 10);
  return Number.isFinite(n) && n >= 0 ? n : 0;
}

function sliceListResponse(response, limit, offset) {
  const total = response.data.length;
  const start = Math.min(offset, total);
  const end = Math.min(start + limit, total);
  const data = response.data.slice(start, end);
  const nextOffset = end < total ? end : null;
  const prevOffset = start > 0 ? Math.max(0, start - limit) : null;
  return {
    data,
    page: {
      limit,
      next_cursor: nextOffset != null ? String(nextOffset) : null,
      prev_cursor: prevOffset != null ? String(prevOffset) : null,
    },
  };
}

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
    const limit = clampLimit(url.searchParams.get('limit'));
    const offset = parseCursor(url.searchParams.get('cursor'));
    send(res, 200, sliceListResponse(LIST_RESPONSE, limit, offset));
    return;
  }

  if (path === '/v1/ledgers') {
    const limit = clampLimit(url.searchParams.get('limit'));
    const offset = parseCursor(url.searchParams.get('cursor'));
    const order = url.searchParams.get('order') === 'asc' ? 'asc' : 'desc';
    // Toggle direction by reversing the in-memory pool before slicing.
    // The cursor is offset-based so each (order, cursor) pair points at
    // a deterministic page — frontend resets the cursor on order change.
    const source =
      order === 'asc'
        ? {
            ...LEDGER_LIST_RESPONSE,
            data: [...LEDGER_LIST_RESPONSE.data].reverse(),
          }
        : LEDGER_LIST_RESPONSE;
    send(res, 200, sliceListResponse(source, limit, offset));
    return;
  }

  if (path === '/v1/assets') {
    const limit = clampLimit(url.searchParams.get('limit'));
    const offset = parseCursor(url.searchParams.get('cursor'));
    const order = url.searchParams.get('order') === 'asc' ? 'asc' : 'desc';
    const codeFilter = (url.searchParams.get('filter[code]') ?? '')
      .trim()
      .toUpperCase();
    const typeFilter = url.searchParams.get('filter[type]') ?? '';
    let filtered = ASSET_FIXTURES;
    if (codeFilter)
      filtered = filtered.filter((a) =>
        (a.asset_code ?? '').toUpperCase().includes(codeFilter)
      );
    if (typeFilter)
      filtered = filtered.filter((a) => a.asset_type_name === typeFilter);
    // Sort by total_supply (DESC default to match Figma chevron).
    const sorted = [...filtered].sort((a, b) => {
      const sa = Number(a.total_supply ?? 0);
      const sb = Number(b.total_supply ?? 0);
      return order === 'asc' ? sa - sb : sb - sa;
    });
    send(res, 200, sliceListResponse({ data: sorted }, limit, offset));
    return;
  }

  const assetTxMatch = /^\/v1\/assets\/([^/]+)\/transactions$/.exec(path);
  if (assetTxMatch) {
    const limit = clampLimit(url.searchParams.get('limit'));
    const offset = parseCursor(url.searchParams.get('cursor'));
    const order = url.searchParams.get('order') === 'asc' ? 'asc' : 'desc';
    const base = LIST_RESPONSE.data;
    const pool = order === 'asc' ? [...base].reverse() : base;
    send(res, 200, sliceListResponse({ data: pool }, limit, offset));
    return;
  }

  const assetMatch = /^\/v1\/assets\/([^/]+)$/.exec(path);
  if (assetMatch) {
    const asset = findAsset(assetMatch[1]);
    if (asset == null) {
      notFound(res, 'asset_not_found', 'Asset not indexed');
      return;
    }
    send(res, 200, asset);
    return;
  }

  // Account StrKeys are CAP-23 base32 (G + 55 chars from [A-Z2-7]) but
  // the synthetic ACCOUNT_POOL fixtures include arbitrary alphanumerics
  // for visual variety, so accept a permissive G + 55 alphanumerics.
  const accountTxMatch =
    /^\/v1\/accounts\/(G[A-Z0-9]{55})\/transactions$/i.exec(path);
  if (accountTxMatch) {
    const accountId = accountTxMatch[1];
    // Empty fixture (Figma node 157:22228) — drives the "No transactions
    // yet" empty state. Any all-A account triggers it.
    if (accountId === EMPTY_ACCOUNT_ID) {
      send(res, 200, sliceListResponse({ data: [] }, 20, 0));
      return;
    }
    const limit = clampLimit(url.searchParams.get('limit'));
    const offset = parseCursor(url.searchParams.get('cursor'));
    const order = url.searchParams.get('order') === 'asc' ? 'asc' : 'desc';
    // Pool is desc by created_at (oldest last). Reverse before slicing
    // when ascending — same pattern as `/v1/ledgers`.
    const base = LIST_RESPONSE.data.map((tx) => ({
      ...tx,
      source_account: accountId,
    }));
    const pool = order === 'asc' ? [...base].reverse() : base;
    send(res, 200, sliceListResponse({ data: pool }, limit, offset));
    return;
  }

  const accountMatch = /^\/v1\/accounts\/(G[A-Z0-9]{55})$/i.exec(path);
  if (accountMatch) {
    const accountId = accountMatch[1];
    // Empty fixture (Figma node 157:22228) — same summary header but
    // zero balances + zero recent transactions so the empty states
    // render. Visit
    //   /accounts/GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB6PFY
    // to see them.
    if (accountId === EMPTY_ACCOUNT_ID) {
      send(res, 200, {
        account_id: accountId,
        sequence_number: 0,
        first_seen_ledger: 0,
        last_seen_ledger: 0,
        home_domain: null,
        balances: [],
      });
      return;
    }
    // Mock summary with a couple of balances. Real backend reads from
    // account_balances_current — fixed-precision NUMERIC(28,7) strings,
    // so balances stay strings here too.
    send(res, 200, {
      account_id: accountId,
      sequence_number: 54_821_301_000_000,
      first_seen_ledger: 12_400_100,
      last_seen_ledger: 54_821_301,
      home_domain: null,
      balances: [
        {
          asset_type_name: 'native',
          asset_code: 'XLM',
          asset_issuer: null,
          balance: '24891.5000000',
          last_updated_ledger: 54_821_301,
          type: 0,
        },
        {
          asset_type_name: 'credit_alphanum4',
          asset_code: 'USDC',
          asset_issuer:
            'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
          balance: '1250.0000000',
          last_updated_ledger: 54_821_295,
          type: 1,
        },
        {
          asset_type_name: 'credit_alphanum4',
          asset_code: 'USDC',
          // Contract-issued (Soroban Asset Contract) — issuer is a C-strkey.
          asset_issuer:
            'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC',
          balance: '500.0000000',
          last_updated_ledger: 54_821_290,
          type: 1,
        },
        {
          asset_type_name: 'credit_alphanum4',
          asset_code: 'EURC',
          asset_issuer:
            'GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPPL',
          balance: '320.4500000',
          last_updated_ledger: 54_821_280,
          type: 1,
        },
      ],
    });
    return;
  }

  const ledgerMatch = /^\/v1\/ledgers\/(\d+)$/.exec(path);
  if (ledgerMatch) {
    const sequence = Number(ledgerMatch[1]);
    const header = LEDGER_LIST_RESPONSE.data.find(
      (l) => l.sequence === sequence
    );
    if (header == null) {
      notFound(res, 'ledger_not_found', 'Ledger not indexed');
      return;
    }
    // Embedded paginated transactions — deterministic per ledger so
    // navigating prev/next gives stable rows. Pool is the same synthetic
    // tx generator used for the global list, salted by the sequence.
    const limit = clampLimit(url.searchParams.get('limit'));
    const offset = parseCursor(url.searchParams.get('cursor'));
    const txCount = header.transaction_count;
    const txPool = Array.from({ length: txCount }, (_, i) => {
      const row = buildSyntheticTxRow(((sequence + i) % 90) + 4);
      return { ...row, ledger_sequence: sequence };
    });
    const txSlice = sliceListResponse({ data: txPool }, limit, offset);
    const idx = LEDGER_LIST_RESPONSE.data.indexOf(header);
    send(res, 200, {
      base_fee: header.base_fee,
      closed_at: header.closed_at,
      hash: header.hash,
      protocol_version: header.protocol_version,
      sequence: header.sequence,
      transaction_count: txCount,
      // Pool is desc by sequence (index 0 = newest). prev = older (next
      // in the array), next = newer (previous in the array).
      prev_sequence: LEDGER_LIST_RESPONSE.data[idx + 1]?.sequence ?? null,
      next_sequence: LEDGER_LIST_RESPONSE.data[idx - 1]?.sequence ?? null,
      transactions: txSlice,
    });
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

  if (path === '/v1/liquidity-pools') {
    const limit = clampLimit(url.searchParams.get('limit'));
    const offset = parseCursor(url.searchParams.get('cursor'));
    const assetFilter = (url.searchParams.get('filter[asset_code]') ?? '')
      .trim()
      .toUpperCase();
    const minTvlRaw = url.searchParams.get('filter[min_tvl]') ?? '';
    const minTvl = minTvlRaw ? Number(minTvlRaw) : NaN;
    let filtered = POOL_FIXTURES;
    if (assetFilter) {
      filtered = filtered.filter(
        (p) =>
          (p.asset_a.asset_code ?? '').toUpperCase().includes(assetFilter) ||
          (p.asset_b.asset_code ?? '').toUpperCase().includes(assetFilter)
      );
    }
    if (Number.isFinite(minTvl)) {
      filtered = filtered.filter((p) => Number(p.tvl ?? '0') >= minTvl);
    }
    send(res, 200, sliceListResponse({ data: filtered }, limit, offset));
    return;
  }

  const poolChartMatch = /^\/v1\/liquidity-pools\/([^/]+)\/chart$/.exec(path);
  if (poolChartMatch) {
    const pool = findPool(poolChartMatch[1]);
    if (pool == null) {
      notFound(res, 'pool_not_found', 'Pool not indexed');
      return;
    }
    const interval = url.searchParams.get('interval') ?? '1d';
    const fromParam = url.searchParams.get('from');
    const now = Date.now();
    // Bucket width derived from interval. The frontend hook (1D/7D/30D
    // /1Y) picks `1h`/`1h`/`1d`/`1w` — honour each so toggling presets
    // produces visibly different shapes (different bucket count + cadence).
    const bucketMs =
      interval === '1h'
        ? 60 * 60_000
        : interval === '1w'
        ? 7 * 86_400_000
        : 86_400_000;
    // Window size — `from` arrives as ISO from the hook.
    const parsedFrom = fromParam ? new Date(fromParam).getTime() : NaN;
    const fromMs = Number.isFinite(parsedFrom)
      ? parsedFrom
      : now - 14 * 86_400_000;
    const rawSpan = now - fromMs;
    const span = rawSpan > bucketMs ? rawSpan : bucketMs;
    const bucketCount = Math.min(180, Math.max(2, Math.ceil(span / bucketMs)));
    const baseTvl = Number(pool.tvl ?? '1000000');
    // Each preset gets a different shape so toggling 1D/7D/30D/1Y
    // produces visibly distinct curves rather than a uniform wave:
    //   1h → choppy, near-flat with small noise (intraday)
    //   1d → medium-frequency sine + light noise (weekly cycles)
    //   1w → low-frequency, big trend (seasonal)
    const shape =
      interval === '1h'
        ? { freq: 6, amp: 0.015, drift: 0.01, noise: 0.008, volAmp: 0.6 }
        : interval === '1w'
        ? { freq: 1.5, amp: 0.18, drift: 0.22, noise: 0.04, volAmp: 1.8 }
        : { freq: 3, amp: 0.06, drift: 0.05, noise: 0.015, volAmp: 1.0 };
    // Deterministic pseudo-random per bucket index (no Math.random so
    // identical requests return identical data).
    const noiseAt = (i) => {
      const x = Math.sin(i * 12.9898 + bucketCount * 78.233) * 43758.5453;
      return x - Math.floor(x) - 0.5;
    };
    const data_points = Array.from({ length: bucketCount }, (_, i) => {
      const t = (i / Math.max(bucketCount - 1, 1)) * Math.PI * 2;
      const wave = Math.sin(t * shape.freq) * shape.amp;
      const drift = (i / Math.max(bucketCount - 1, 1)) * shape.drift;
      const noise = noiseAt(i) * shape.noise;
      const tvl = baseTvl * (1 + drift + wave + noise);
      const volMagnitude = 0.5 + Math.abs(wave) * 6 + Math.abs(noise) * 4;
      return {
        bucket: new Date(now - (bucketCount - 1 - i) * bucketMs).toISOString(),
        tvl: tvl.toFixed(2),
        volume: (baseTvl * 0.02 * shape.volAmp * volMagnitude).toFixed(2),
        fee_revenue: (baseTvl * 0.00008 * shape.volAmp * volMagnitude).toFixed(
          2
        ),
        samples_in_bucket: interval === '1h' ? 1 : interval === '1d' ? 24 : 168,
      };
    });
    send(res, 200, {
      pool_id: pool.pool_id,
      interval,
      from: data_points[0].bucket,
      to: new Date(now).toISOString(),
      data_points,
    });
    return;
  }

  const poolParticipantsMatch =
    /^\/v1\/liquidity-pools\/([^/]+)\/participants$/.exec(path);
  if (poolParticipantsMatch) {
    const pool = findPool(poolParticipantsMatch[1]);
    if (pool == null) {
      notFound(res, 'pool_not_found', 'Pool not indexed');
      return;
    }
    const limit = clampLimit(url.searchParams.get('limit'));
    const offset = parseCursor(url.searchParams.get('cursor'));
    // Cap at ACCOUNT_POOL.length so each synthetic participant has a
    // unique `account` strkey — `PoolParticipants` keys rows by account
    // and React warns on duplicate keys (which we'd hit at i >= 11
    // since `buildPoolParticipantRow` cycles `ACCOUNT_POOL`).
    const pool_count = Math.min(pool.participant_count, ACCOUNT_POOL.length);
    const rows = Array.from({ length: pool_count }, (_, i) =>
      buildPoolParticipantRow(i, pool)
    );
    send(res, 200, sliceListResponse({ data: rows }, limit, offset));
    return;
  }

  const poolTransactionsMatch =
    /^\/v1\/liquidity-pools\/([^/]+)\/transactions$/.exec(path);
  if (poolTransactionsMatch) {
    if (findPool(poolTransactionsMatch[1]) == null) {
      notFound(res, 'pool_not_found', 'Pool not indexed');
      return;
    }
    const limit = clampLimit(url.searchParams.get('limit'));
    const offset = parseCursor(url.searchParams.get('cursor'));
    const rows = Array.from({ length: 24 }, (_, i) =>
      buildPoolTransactionRow(i)
    );
    send(res, 200, sliceListResponse({ data: rows }, limit, offset));
    return;
  }

  const poolDetailMatch = /^\/v1\/liquidity-pools\/([^/]+)$/.exec(path);
  if (poolDetailMatch) {
    const pool = findPool(poolDetailMatch[1]);
    if (pool == null) {
      notFound(res, 'pool_not_found', 'Pool not indexed');
      return;
    }
    send(res, 200, pool);
    return;
  }

  if (path === '/v1/nfts') {
    const limit = clampLimit(url.searchParams.get('limit'));
    const offset = parseCursor(url.searchParams.get('cursor'));
    const collectionFilter = (url.searchParams.get('filter[collection]') ?? '')
      .trim()
      .toLowerCase();
    const contractFilter = (
      url.searchParams.get('filter[contract_id]') ?? ''
    ).trim();
    let filtered = NFT_FIXTURES;
    if (collectionFilter) {
      filtered = filtered.filter((n) =>
        (n.collection_name ?? '').toLowerCase().includes(collectionFilter)
      );
    }
    if (contractFilter) {
      filtered = filtered.filter((n) => n.contract_id === contractFilter);
    }
    send(res, 200, sliceListResponse({ data: filtered }, limit, offset));
    return;
  }

  const nftTransfersMatch = /^\/v1\/nfts\/([^/]+)\/([^/]+)\/transfers$/.exec(
    path
  );
  if (nftTransfersMatch) {
    const contractId = nftTransfersMatch[1];
    const tokenId = decodeURIComponent(nftTransfersMatch[2]);
    if (findNftToken(contractId, tokenId) == null) {
      notFound(res, 'nft_not_found', 'NFT not indexed');
      return;
    }
    const limit = clampLimit(url.searchParams.get('limit'));
    const offset = parseCursor(url.searchParams.get('cursor'));
    const pool = Array.from({ length: 24 }, (_, i) =>
      buildNftTransferRow(i, contractId, tokenId)
    );
    send(res, 200, sliceListResponse({ data: pool }, limit, offset));
    return;
  }

  const nftDetailMatch = /^\/v1\/nfts\/([^/]+)\/([^/]+)$/.exec(path);
  if (nftDetailMatch) {
    const contractId = nftDetailMatch[1];
    const tokenId = decodeURIComponent(nftDetailMatch[2]);
    const nft = findNftToken(contractId, tokenId);
    if (nft == null) {
      notFound(res, 'nft_not_found', 'NFT not indexed');
      return;
    }
    send(res, 200, {
      ...nft,
      // Off-chain JSON metadata — Figma trait card grid expects the
      // OpenSea-style `attributes` array. `rarity_percent` per entry
      // drives the "X% have this" sub-line.
      metadata: {
        name: nft.name,
        description: `${nft.collection_name} collectible #${nft.token_id} on Stellar.`,
        image: nft.media_url,
        attributes: NFT_TRAITS_TEMPLATE,
      },
    });
    return;
  }

  const contractInvocationsMatch =
    /^\/v1\/contracts\/([^/]+)\/invocations$/.exec(path);
  if (contractInvocationsMatch) {
    const id = contractInvocationsMatch[1];
    if (findContract(id) == null) {
      notFound(res, 'contract_not_found', 'Contract not indexed');
      return;
    }
    const limit = clampLimit(url.searchParams.get('limit'));
    const offset = parseCursor(url.searchParams.get('cursor'));
    const pool = Array.from({ length: 72 }, (_, i) =>
      buildContractInvocationRow(i, id)
    );
    send(res, 200, sliceListResponse({ data: pool }, limit, offset));
    return;
  }

  const contractEventsMatch = /^\/v1\/contracts\/([^/]+)\/events$/.exec(path);
  if (contractEventsMatch) {
    const id = contractEventsMatch[1];
    if (findContract(id) == null) {
      notFound(res, 'contract_not_found', 'Contract not indexed');
      return;
    }
    const limit = clampLimit(url.searchParams.get('limit'));
    const offset = parseCursor(url.searchParams.get('cursor'));
    const pool = Array.from({ length: 72 }, (_, i) =>
      buildContractEventRow(i, id)
    );
    send(res, 200, sliceListResponse({ data: pool }, limit, offset));
    return;
  }

  const contractInterfaceMatch = /^\/v1\/contracts\/([^/]+)\/interface$/.exec(
    path
  );
  if (contractInterfaceMatch) {
    const contract = findContract(contractInterfaceMatch[1]);
    if (contract == null) {
      notFound(res, 'contract_not_found', 'Contract not indexed');
      return;
    }
    send(res, 200, {
      contract_id: contract.contract_id,
      interface_metadata: contract.interface,
      wasm_hash: contract.wasm_hash,
    });
    return;
  }

  const contractMatch = /^\/v1\/contracts\/([^/]+)$/.exec(path);
  if (contractMatch) {
    const contract = findContract(contractMatch[1]);
    if (contract == null) {
      notFound(res, 'contract_not_found', 'Contract not indexed');
      return;
    }
    // Drop the synthetic `interface` field — that's only used by the
    // `/interface` sub-route, never echoed back on the main detail.
    const { interface: _iface, ...rest } = contract;
    send(res, 200, rest);
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
