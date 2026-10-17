// @vitest-environment node
import {
  Account,
  Address,
  MuxedAccount,
  Networks,
  SorobanDataBuilder,
  TransactionBuilder,
  nativeToScVal,
  rpc,
  xdr,
  type Transaction,
} from '@stellar/stellar-sdk';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  decodeSoranDestination,
  parseSoranName,
  resolveSoranPayment,
  resolveSoranPrimary,
} from './soran.js';

const ACCOUNT = 'GBHKTFVBDUA6RYP5JM4SPZ76OXYAAHV4QHUOFV4S4TK342FMVGPHA2WN';
const CONTRACT = 'CCVKI6UYJDO34LO4D653IXCTPBGJOHJSJGSIOB4A46IH4ULNHS2MPQL7';
const MUXED_BASE = 'GDHHA2WBSH4ZKIAWALPY4KVOC57ZUT6W6HWS3JBUQ4KFJRT6US4MXTZ5';
const MUXED =
  'MDHHA2WBSH4ZKIAWALPY4KVOC57ZUT6W6HWS3JBUQ4KFJRT6US4MWAAAAAAAAAAAFKV2W';
const LOOKUP = 'CDSORANQAJK35UV2HR63CMB6M5NYISHMUBTB6EQY2CZ3Y7HJDIOHRJWA';
const REGISTRY = 'CCSORANDPQINYOYB5SVO45WJP2LBBYKC72HHUIRVXB4J6RUZKDAUW7G4';
const PRIMARY = 'CCSORAN7Y7ICQK2MBSVCJT3BUN5EHXKDKSTMGVB6QWSYXWMMLG2WIFJ6';

const symbol = (value: string) => nativeToScVal(value, { type: 'symbol' });
const string = (value: string) => nativeToScVal(value, { type: 'string' });
const address = (value: string) => nativeToScVal(value, { type: 'address' });
const u64 = (value: bigint) => nativeToScVal(value, { type: 'u64' });
const variant = (tag: string, ...args: xdr.ScVal[]) =>
  xdr.ScVal.scvVec([symbol(tag), ...args]);
const fields = (...values: [string, xdr.ScVal][]) =>
  xdr.ScVal.scvMap(
    values.map(([key, val]) => new xdr.ScMapEntry({ key: symbol(key), val }))
  );
const direct = (destination = ACCOUNT, memo: xdr.ScVal = variant('None')) =>
  variant('Direct', fields(['address', address(destination)], ['memo', memo]));
const muxed = (destination = MUXED_BASE, id = 42n) =>
  variant('Muxed', fields(['account', address(destination)], ['id', u64(id)]));

function success(
  retval: xdr.ScVal
): rpc.Api.SimulateTransactionSuccessResponse {
  return {
    id: '1',
    latestLedger: 5039355,
    events: [],
    _parsed: true,
    transactionData: new SorobanDataBuilder(),
    minResourceFee: '0',
    result: { auth: [], retval },
  };
}

function failure(error: string): rpc.Api.SimulateTransactionErrorResponse {
  return { id: '1', latestLedger: 5039355, events: [], _parsed: true, error };
}

function invocation(tx: Parameters<rpc.Server['simulateTransaction']>[0]) {
  const op = (tx as Transaction).operations[0];
  if (
    op.type !== 'invokeHostFunction' ||
    op.func.type !== 'hostFunctionTypeInvokeContract'
  )
    throw new Error('Expected contract invocation');
  return op.func.invokeContract;
}

function methodOf(
  tx: Parameters<rpc.Server['simulateTransaction']>[0]
): string {
  return invocation(tx).functionName.toStringStrict();
}

function required<T>(value: T | undefined): T {
  if (value === undefined) throw new Error('Expected fixture value');
  return value;
}

function mockReads(
  overrides: Record<string, rpc.Api.SimulateTransactionResponse | Error> = {}
) {
  const defaults: Record<string, rpc.Api.SimulateTransactionResponse> = {
    registry: success(address(REGISTRY)),
    version: success(xdr.ScVal.scvU32(2)),
    destination_version: success(xdr.ScVal.scvU32(2)),
    subname_version: success(xdr.ScVal.scvU32(1)),
    primary: success(address(PRIMARY)),
    muxed_identity_version: success(xdr.ScVal.scvU32(1)),
    resolve_destination: success(direct()),
    primary_name: success(string('robert.nova')),
    primary_name_muxed: success(string('mux.nova')),
  };
  return vi
    .spyOn(rpc.Server.prototype, 'simulateTransaction')
    .mockImplementation(async (tx) => {
      const method = methodOf(tx);
      const response = overrides[method] ?? defaults[method];
      if (response instanceof Error) throw response;
      if (!response) throw new Error(`Unexpected method ${method}`);
      return response;
    });
}

function pendingRpcFetch() {
  const fetchMock = vi.fn(
    (_url: RequestInfo | URL, init?: RequestInit) =>
      new Promise<Response>((_resolve, reject) => {
        const signal = init?.signal;
        if (signal?.aborted) reject(signal.reason);
        else
          signal?.addEventListener('abort', () => reject(signal.reason), {
            once: true,
          });
      })
  );
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe('parseSoranName', () => {
  it('canonicalizes ASCII case and supports any namespace plus one child level', () => {
    expect(parseSoranName('Alice.NOVA')).toEqual({
      name: 'alice.nova',
      namespace: 'nova',
      label: 'alice',
    });
    expect(parseSoranName('Pay.Alice.my-brand')).toEqual({
      name: 'pay.alice.my-brand',
      namespace: 'my-brand',
      label: 'pay.alice',
    });
    expect(parseSoranName('a.1')).toEqual({
      name: 'a.1',
      namespace: '1',
      label: 'a',
    });
    const label = 'a'.repeat(63);
    expect(parseSoranName(`${label}.${label}.${label}`)).not.toBeNull();
  });

  it.each([
    '',
    'nova',
    '.nova',
    'alice.',
    'a..nova',
    'one.two.three.nova',
    '-alice.nova',
    'alice-.nova',
    ' alice.nova',
    'alice.nova ',
    'alice.nova\n',
    'al ice.nova',
    'alice/nova',
    'alice*nova',
    'Kate.nova',
    'álîce.nova',
    'аlice.nova',
    `${'a'.repeat(64)}.nova`,
  ])('rejects invalid input %j', (input) => {
    expect(parseSoranName(input)).toBeNull();
  });
});

describe('decodeSoranDestination', () => {
  it('decodes the published Lookup fixture without reconstructing its ABI', () => {
    // Soran lookup fixture direct-g-id-max, also used by the reference explorer PR.
    const fixture =
      'AAAAEAAAAAEAAAACAAAADwAAAAZEaXJlY3QAAAAAABEAAAABAAAAAgAAAA8AAAAHYWRkcmVzcwAAAAASAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA8AAAAEbWVtbwAAABAAAAABAAAAAgAAAA8AAAACSWQAAAAAAAX//////////w==';
    expect(
      decodeSoranDestination(xdr.ScVal.fromXDR(fixture, 'base64'))
    ).toEqual({
      address: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      memo: { type: 'id', value: '18446744073709551615' },
    });
  });
  it.each([ACCOUNT, CONTRACT])('decodes direct memo-free %s', (value) => {
    expect(decodeSoranDestination(direct(value))).toEqual({
      address: value,
      memo: { type: 'none' },
    });
  });

  it.each([0n, 12345n, 18446744073709551615n])(
    'preserves the complete u64 memo %s',
    (id) => {
      expect(
        decodeSoranDestination(direct(ACCOUNT, variant('Id', u64(id))))
      ).toEqual({
        address: ACCOUNT,
        memo: { type: 'id', value: id.toString() },
      });
    }
  );

  it.each(['hello', '  memo  ', '\uFEFFmemo', 'é'.repeat(14), '🙂'.repeat(7)])(
    'preserves exact UTF-8 memo text %j',
    (text) => {
      expect(
        decodeSoranDestination(direct(ACCOUNT, variant('Text', string(text))))
      ).toEqual({ address: ACCOUNT, memo: { type: 'text', value: text } });
    }
  );

  it('decodes exactly 32 bytes of hash memo without hashing it again', () => {
    const bytes = Uint8Array.from({ length: 32 }, (_, i) => i);
    expect(
      decodeSoranDestination(
        direct(ACCOUNT, variant('Hash', nativeToScVal(bytes)))
      )
    ).toEqual({
      address: ACCOUNT,
      memo: {
        type: 'hash',
        value:
          '000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f',
      },
    });
  });

  it.each([0n, 42n, 18446744073709551615n])(
    'preserves the complete muxed address for u64 ID %s',
    (id) => {
      const expected = new MuxedAccount(
        new Account(MUXED_BASE, '0'),
        id.toString()
      ).accountId();
      expect(decodeSoranDestination(muxed(MUXED_BASE, id))).toEqual({
        address: expected,
        memo: { type: 'none' },
      });
    }
  );

  it.each([
    ['empty enum', xdr.ScVal.scvVec([])],
    [
      'string enum tag',
      xdr.ScVal.scvVec([
        string('Direct'),
        fields(['address', address(ACCOUNT)], ['memo', variant('None')]),
      ]),
    ],
    ['unknown variant', variant('LegacyAddress', address(ACCOUNT))],
    ['no destination argument', variant('Direct')],
    ['two destination arguments', variant('Direct', fields(), fields())],
    ['missing memo', variant('Direct', fields(['address', address(ACCOUNT)]))],
    [
      'reordered fields',
      variant(
        'Direct',
        fields(['memo', variant('None')], ['address', address(ACCOUNT)])
      ),
    ],
    [
      'duplicate fields',
      variant(
        'Direct',
        fields(['address', address(ACCOUNT)], ['address', address(ACCOUNT)])
      ),
    ],
    [
      'string address',
      variant(
        'Direct',
        fields(['address', string(ACCOUNT)], ['memo', variant('None')])
      ),
    ],
    ['memo none with a value', direct(ACCOUNT, variant('None', u64(1n)))],
    ['memo void instead of enum', direct(ACCOUNT, xdr.ScVal.scvVoid())],
    [
      'numeric memo wrong wire type',
      direct(ACCOUNT, variant('Id', xdr.ScVal.scvU32(12))),
    ],
    [
      'memo text wrong wire type',
      direct(ACCOUNT, variant('Text', symbol('memo'))),
    ],
    ['empty text', direct(ACCOUNT, variant('Text', string('')))],
    [
      'text beyond byte limit',
      direct(ACCOUNT, variant('Text', string('é'.repeat(15)))),
    ],
    [
      'invalid UTF-8',
      direct(
        ACCOUNT,
        variant('Text', xdr.ScVal.scvString(new Uint8Array([255])))
      ),
    ],
    [
      'short hash',
      direct(ACCOUNT, variant('Hash', nativeToScVal(new Uint8Array(31)))),
    ],
    [
      'unknown memo',
      direct(ACCOUNT, variant('Return', nativeToScVal(new Uint8Array(32)))),
    ],
    ['C contract with memo', direct(CONTRACT, variant('Id', u64(42n)))],
    ['muxed C base', muxed(CONTRACT)],
    [
      'muxed ID wrong wire type',
      variant(
        'Muxed',
        fields(['account', address(ACCOUNT)], ['id', string('42')])
      ),
    ],
  ])('rejects %s', (_label, value) => {
    expect(() => decodeSoranDestination(value)).toThrow();
  });
});

describe('resolveSoranPayment', () => {
  it('checks deployed anchors and ABI before reading the canonical name', async () => {
    const simulate = mockReads({
      resolve_destination: success(direct(ACCOUNT, variant('Id', u64(12345n)))),
    });
    const send = vi.spyOn(rpc.Server.prototype, 'sendTransaction');
    await expect(
      resolveSoranPayment('Robert.NOVA', 'testnet')
    ).resolves.toEqual({
      kind: 'resolved',
      payment: { address: ACCOUNT, memo: { type: 'id', value: '12345' } },
    });
    const methods = simulate.mock.calls.map(([tx]) => methodOf(tx));
    expect(methods.slice(0, 3).sort()).toEqual([
      'destination_version',
      'registry',
      'version',
    ]);
    expect(methods[3]).toBe('resolve_destination');
    const [tx] = simulate.mock.calls[3];
    expect((tx as Transaction).networkPassphrase).toBe(Networks.TESTNET);
    expect((tx as Transaction).signatures).toHaveLength(0);
    expect((tx as Transaction).operations).toHaveLength(1);
    expect(
      Address.fromScVal(
        xdr.ScVal.scvAddress(invocation(tx).contractAddress)
      ).toString()
    ).toBe(LOOKUP);
    expect(invocation(tx).args[0]).toEqual(string('robert.nova'));
    expect(send).not.toHaveBeenCalled();
  });

  it('checks subname support before reading a child', async () => {
    const simulate = mockReads();
    await expect(
      resolveSoranPayment('Pay.Robert.Nova', 'testnet')
    ).resolves.toMatchObject({ kind: 'resolved' });
    const methods = simulate.mock.calls.map(([tx]) => methodOf(tx));
    expect(methods.indexOf('subname_version')).toBeLessThan(
      methods.indexOf('resolve_destination')
    );
    expect(invocation(required(simulate.mock.calls.at(-1))[0]).args[0]).toEqual(
      string('pay.robert.nova')
    );
  });

  it.each([
    ['registry', success(address(CONTRACT))],
    ['registry', success(string(REGISTRY))],
    ['version', success(xdr.ScVal.scvU32(1))],
    ['version', success(u64(2n))],
    ['destination_version', success(xdr.ScVal.scvU32(1))],
    ['registry', failure('HostError: Error(Contract, #5)')],
    ['registry', failure('HostError: Error(Contract, #7)')],
    ['registry', success(xdr.ScVal.scvError(xdr.ScError.sceContract(5)))],
    ['registry', success(xdr.ScVal.scvError(xdr.ScError.sceContract(7)))],
  ])('fails closed on a bad %s check', async (method, result) => {
    const simulate = mockReads({ [method]: result });
    const resolved = await resolveSoranPayment('robert.nova', 'testnet');
    expect(resolved).toMatchObject({ kind: 'failed' });
    expect(resolved).not.toHaveProperty('code');
    expect(simulate.mock.calls.map(([tx]) => methodOf(tx))).not.toContain(
      'resolve_destination'
    );
  });

  it('rejects unsupported subnames before their payment read', async () => {
    const simulate = mockReads({
      subname_version: success(xdr.ScVal.scvU32(0)),
    });
    await expect(
      resolveSoranPayment('pay.robert.nova', 'testnet')
    ).resolves.toMatchObject({ kind: 'failed' });
    expect(simulate.mock.calls.map(([tx]) => methodOf(tx))).not.toContain(
      'resolve_destination'
    );
  });

  it('makes no request for mainnet, invalid names or an already cancelled caller', async () => {
    const simulate = mockReads();
    const caller = new AbortController();
    caller.abort();
    await expect(
      resolveSoranPayment('robert.nova', 'mainnet')
    ).resolves.toMatchObject({ kind: 'failed' });
    await expect(
      resolveSoranPayment('bad name', 'testnet')
    ).resolves.toMatchObject({ kind: 'failed' });
    await expect(
      resolveSoranPayment('robert.nova', 'testnet', caller.signal)
    ).resolves.toMatchObject({ kind: 'failed' });
    expect(simulate).not.toHaveBeenCalled();
  });

  it('never uses a simulated result when archived state requires restoration', async () => {
    const simulated = success(direct());
    const simulate = mockReads({
      resolve_destination: {
        ...simulated,
        result: required(simulated.result),
        restorePreamble: {
          transactionData: new SorobanDataBuilder(),
          minResourceFee: '100',
        },
      },
    });
    await expect(
      resolveSoranPayment('robert.nova', 'testnet')
    ).resolves.toMatchObject({
      kind: 'failed',
      reason: expect.stringContaining('restor'),
    });
    expect(simulate.mock.calls.map(([tx]) => methodOf(tx))).toEqual([
      'registry',
      'version',
      'destination_version',
      'resolve_destination',
    ]);
  });

  it.each([5, 7])(
    'marks Lookup error %i as no active destination in both error representations',
    async (code) => {
      for (const response of [
        failure(`HostError: Error(Contract, #${code})\n\nEvent log`),
        success(xdr.ScVal.scvError(xdr.ScError.sceContract(code))),
      ]) {
        vi.restoreAllMocks();
        mockReads({ resolve_destination: response });
        await expect(
          resolveSoranPayment('missing.nova', 'testnet')
        ).resolves.toEqual({
          kind: 'failed',
          code: 'not_found',
          reason:
            code === 5
              ? 'Soran namespace was not found on Testnet.'
              : 'This Soran name has no active destination on Testnet.',
        });
      }
    }
  );

  it.each([1, 2, 3, 4, 6, 8, 9, 10, 11, 12, 999])(
    'does not classify Lookup error %i as a search miss',
    async (code) => {
      for (const response of [
        failure(`HostError: Error(Contract, #${code})`),
        success(xdr.ScVal.scvError(xdr.ScError.sceContract(code))),
      ]) {
        vi.restoreAllMocks();
        mockReads({ resolve_destination: response });
        const resolved = await resolveSoranPayment('robert.nova', 'testnet');
        expect(resolved).toMatchObject({ kind: 'failed' });
        expect(resolved).not.toHaveProperty('code');
      }
    }
  );

  it.each([5, 7])(
    'does not classify nested diagnostic code %i or transport prose as a search miss',
    async (code) => {
      for (const response of [
        failure(
          `HostError: Error(Storage, MissingValue)\nEvent log: Error(Contract, #${code})`
        ),
        new Error(`HostError: Error(Contract, #${code})`),
      ]) {
        vi.restoreAllMocks();
        mockReads({ resolve_destination: response });
        const resolved = await resolveSoranPayment('robert.nova', 'testnet');
        expect(resolved).toMatchObject({ kind: 'failed' });
        expect(resolved).not.toHaveProperty('code');
      }
    }
  );

  it('rejects missing returns, invalid destinations and transport failures', async () => {
    const missing = success(direct());
    delete missing.result;
    for (const result of [
      missing,
      success(string(ACCOUNT)),
      new Error('Network offline'),
    ]) {
      vi.restoreAllMocks();
      mockReads({ resolve_destination: result });
      const resolved = await resolveSoranPayment('robert.nova', 'testnet');
      expect(resolved).toMatchObject({ kind: 'failed' });
      expect(resolved).not.toHaveProperty('code');
    }
  });

  it('preserves privacy and caller cancellation through the SDK transport', async () => {
    const fetchMock = pendingRpcFetch();
    const caller = new AbortController();
    const pending = resolveSoranPayment(
      'robert.nova',
      'testnet',
      caller.signal
    );
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(3));
    caller.abort();
    await expect(pending).resolves.toMatchObject({ kind: 'failed' });
    for (const [url, options] of fetchMock.mock.calls) {
      expect(new URL(String(url)).origin).toBe(
        'https://soroban-testnet.stellar.org'
      );
      expect(options).toMatchObject({
        method: 'POST',
        cache: 'no-store',
        credentials: 'omit',
        referrerPolicy: 'no-referrer',
      });
      expect(['error', 'manual']).toContain(options?.redirect);
      expect(options?.signal?.aborted).toBe(true);
      expect(JSON.parse(String(options?.body)).method).toBe(
        'simulateTransaction'
      );
    }
  });

  it('bounds the entire operation to fifteen seconds', async () => {
    vi.useFakeTimers();
    const fetchMock = pendingRpcFetch();
    const pending = resolveSoranPayment('robert.nova', 'testnet');
    await vi.advanceTimersByTimeAsync(14_999);
    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(
      fetchMock.mock.calls.every(
        ([, options]) => options?.signal?.aborted === false
      )
    ).toBe(true);
    await vi.advanceTimersByTimeAsync(1);
    await expect(pending).resolves.toMatchObject({ kind: 'failed' });
    expect(
      fetchMock.mock.calls.every(([, options]) => options?.signal?.aborted)
    ).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('decodes raw RPC XDR responses through the SDK without submitting transactions', async () => {
    const returns: Record<string, xdr.ScVal> = {
      registry: address(REGISTRY),
      version: xdr.ScVal.scvU32(2),
      destination_version: xdr.ScVal.scvU32(2),
      resolve_destination: direct(ACCOUNT, variant('Id', u64(12345n))),
    };
    const fetchMock = vi.fn(
      async (_url: RequestInfo | URL, init?: RequestInit) => {
        const request = JSON.parse(String(init?.body));
        expect(request.method).toBe('simulateTransaction');
        const transaction = TransactionBuilder.fromXDR(
          request.params.transaction,
          Networks.TESTNET
        );
        const retval = returns[methodOf(transaction)];
        if (!retval) throw new Error('Unexpected contract read');
        return new Response(
          JSON.stringify({
            jsonrpc: '2.0',
            id: request.id,
            result: {
              latestLedger: 5039355,
              transactionData: new SorobanDataBuilder().build().toXDR('base64'),
              minResourceFee: '0',
              events: [],
              results: [{ auth: [], xdr: retval.toXDR('base64') }],
            },
          }),
          { headers: { 'Content-Type': 'application/json' } }
        );
      }
    );
    vi.stubGlobal('fetch', fetchMock);
    await expect(
      resolveSoranPayment('robert.nova', 'testnet')
    ).resolves.toEqual({
      kind: 'resolved',
      payment: { address: ACCOUNT, memo: { type: 'id', value: '12345' } },
    });
    expect(fetchMock).toHaveBeenCalledTimes(4);
    for (const [url] of fetchMock.mock.calls) {
      expect(new URL(String(url)).origin).toBe(
        'https://soroban-testnet.stellar.org'
      );
    }
  });

  it.each([
    '<html>Unavailable</html>',
    '{}',
    '{"jsonrpc":"2.0","result":null}',
  ])('fails closed on a malformed raw RPC response: %s', async (body) => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(body, {
            headers: { 'Content-Type': 'application/json' },
          })
      )
    );
    await expect(
      resolveSoranPayment('robert.nova', 'testnet')
    ).resolves.toMatchObject({ kind: 'failed' });
  });
});

describe('resolveSoranPrimary', () => {
  it('checks the configured Primary and returns a canonical account election', async () => {
    const simulate = mockReads();
    await expect(resolveSoranPrimary(ACCOUNT, 'testnet')).resolves.toBe(
      'robert.nova'
    );
    const methods = simulate.mock.calls.map(([tx]) => methodOf(tx));
    expect(methods).toContain('primary');
    expect(methods.indexOf('primary')).toBeLessThan(
      methods.indexOf('primary_name')
    );
    expect(invocation(required(simulate.mock.calls.at(-1))[0]).args).toEqual([
      address(ACCOUNT),
    ]);
  });

  it('decodes true Option None as void for a contract with no elected name', async () => {
    mockReads({ primary_name: success(xdr.ScVal.scvVoid()) });
    await expect(resolveSoranPrimary(CONTRACT, 'testnet')).resolves.toBeNull();
  });

  it('checks the M capability and sends the exact base account and u64 ID', async () => {
    const simulate = mockReads();
    await expect(resolveSoranPrimary(MUXED, 'testnet')).resolves.toBe(
      'mux.nova'
    );
    const methods = simulate.mock.calls.map(([tx]) => methodOf(tx));
    expect(methods).toContain('muxed_identity_version');
    expect(methods).not.toContain('primary_name');
    expect(invocation(required(simulate.mock.calls.at(-1))[0]).args).toEqual([
      address(MUXED_BASE),
      u64(42n),
    ]);
  });

  it.each([
    ['primary', success(address(CONTRACT))],
    ['primary', success(xdr.ScVal.scvVoid())],
    ['primary_name', success(symbol('robert.nova'))],
    ['primary_name', success(string('Robert.Nova'))],
    ['primary_name', success(string('robert.nova\n'))],
    ['primary_name', success(string('rоbert.nova'))],
    ['primary_name', success(variant('None'))],
    ['primary_name', success(variant('Some', string('robert.nova')))],
    ['primary_name', failure('HostError: Error(Contract, #7)')],
    ['primary_name', new Error('Network error')],
  ])('withholds display names after a bad %s read', async (method, result) => {
    mockReads({ [method]: result });
    await expect(resolveSoranPrimary(ACCOUNT, 'testnet')).resolves.toBeNull();
  });

  it('never downgrades an unsupported M identity to its G account', async () => {
    const simulate = mockReads({
      muxed_identity_version: success(xdr.ScVal.scvU32(0)),
    });
    await expect(resolveSoranPrimary(MUXED, 'testnet')).resolves.toBeNull();
    expect(simulate.mock.calls.map(([tx]) => methodOf(tx))).not.toContain(
      'primary_name'
    );
    expect(simulate.mock.calls.map(([tx]) => methodOf(tx))).not.toContain(
      'primary_name_muxed'
    );
  });

  it('makes no reads for unsupported network or malformed identities', async () => {
    const simulate = mockReads();
    await expect(resolveSoranPrimary(ACCOUNT, 'mainnet')).resolves.toBeNull();
    await expect(resolveSoranPrimary('invalid', 'testnet')).resolves.toBeNull();
    expect(simulate).not.toHaveBeenCalled();
  });
});
