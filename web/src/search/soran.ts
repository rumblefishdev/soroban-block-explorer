import {
  Account,
  Address,
  Contract,
  MuxedAccount,
  Networks,
  StrKey,
  TransactionBuilder,
  nativeToScVal,
  rpc,
  type xdr,
} from '@stellar/stellar-sdk';

import type { Network } from '../network.js';

// Public Testnet deployment. Anchors and ABI checks are compatibility checks,
// not code hashes: these contracts can be upgraded by their administrators.
const RPC_URL = 'https://soroban-testnet.stellar.org';
const LOOKUP = 'CDSORANQAJK35UV2HR63CMB6M5NYISHMUBTB6EQY2CZ3Y7HJDIOHRJWA';
const REGISTRY = 'CCSORANDPQINYOYB5SVO45WJP2LBBYKC72HHUIRVXB4J6RUZKDAUW7G4';
const PRIMARY = 'CCSORAN7Y7ICQK2MBSVCJT3BUN5EHXKDKSTMGVB6QWSYXWMMLG2WIFJ6';
const SIMULATION_ACCOUNT =
  'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';
const TIMEOUT_MS = 15_000;
const LABEL = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/;

export type SoranMemo =
  | { type: 'none' }
  | { type: 'id' | 'text' | 'hash'; value: string };

export type SoranPayment = { address: string; memo: SoranMemo };

export type SoranResolve =
  | { kind: 'resolved'; payment: SoranPayment }
  // not_found means no current active destination, not an available name.
  | { kind: 'failed'; reason: string; code?: 'not_found' };

/** Two ASCII labels, or one child level. Namespaces are not a fixed suffix list.
 * https://docs.soran.domains/concepts/names-and-namespaces */
export function parseSoranName(
  q: string
): { name: string; namespace: string; label: string } | null {
  if (q.length === 0 || q.length > 191 || /[^A-Za-z0-9.-]/.test(q)) return null;
  const name = q.toLowerCase();
  const parts = name.split('.');
  if (
    (parts.length !== 2 && parts.length !== 3) ||
    !parts.every((part) => LABEL.test(part))
  ) {
    return null;
  }
  return {
    name,
    namespace: parts[parts.length - 1],
    label: parts.slice(0, -1).join('.'),
  };
}

class SoranReadError extends Error {
  constructor(message: string, readonly code?: 'not_found') {
    super(message);
  }
}

function invalidResult(): never {
  throw new SoranReadError(
    'Soran returned invalid payment instructions for this name.'
  );
}

function readSymbol(value: xdr.ScVal): string {
  if (value?.type !== 'scvSymbol') invalidResult();
  return value.sym.toStringStrict();
}

function readVariant(value: xdr.ScVal): [string, xdr.ScVal[]] {
  if (value?.type !== 'scvVec' || !value.vec?.length) invalidResult();
  const [tag, ...args] = value.vec;
  return [readSymbol(tag), args];
}

function readStruct(
  value: xdr.ScVal,
  fields: string[]
): Record<string, xdr.ScVal> {
  if (value?.type !== 'scvMap' || value.map?.length !== fields.length)
    invalidResult();
  const result: Record<string, xdr.ScVal> = {};
  for (let i = 0; i < fields.length; i++) {
    const entry = value.map[i];
    if (readSymbol(entry.key) !== fields[i]) invalidResult();
    result[fields[i]] = entry.val;
  }
  return result;
}

function readAddress(value: xdr.ScVal, accountOnly = false): string {
  if (value?.type !== 'scvAddress') invalidResult();
  const address = Address.fromScVal(value).toString();
  if (
    !StrKey.isValidEd25519PublicKey(address) &&
    (accountOnly || !StrKey.isValidContract(address))
  )
    invalidResult();
  return address;
}

function readId(value: xdr.ScVal): string {
  if (
    value?.type !== 'scvU64' ||
    typeof value.u64 !== 'bigint' ||
    value.u64 < 0n ||
    value.u64 > 18446744073709551615n
  )
    invalidResult();
  return value.u64.toString();
}

function readMemo(value: xdr.ScVal): SoranMemo {
  const [type, args] = readVariant(value);
  if (type === 'None' && !args.length) return { type: 'none' };
  if (args.length !== 1) invalidResult();
  const [memo] = args;
  switch (type) {
    case 'Id':
      return { type: 'id', value: readId(memo) };
    case 'Text':
      // XdrString.length measures bytes, not JavaScript UTF-16 code units.
      if (
        memo.type !== 'scvString' ||
        memo.str.length < 1 ||
        memo.str.length > 28
      )
        invalidResult();
      return { type: 'text', value: memo.str.toStringStrict() };
    case 'Hash': {
      if (memo.type !== 'scvBytes') invalidResult();
      const bytes = memo.bytes.toBytes();
      if (bytes.length !== 32) invalidResult();
      return {
        type: 'hash',
        value: Array.from(bytes, (byte) =>
          byte.toString(16).padStart(2, '0')
        ).join(''),
      };
    }
    default:
      return invalidResult();
  }
}

/** Decode only PaymentDestination ABI 2. Never turn a malformed memo into a
 * memo-free address, or drop a muxed account's exact unsigned 64-bit ID. */
export function decodeSoranDestination(value: xdr.ScVal): SoranPayment {
  try {
    const [type, args] = readVariant(value);
    if (args.length !== 1) invalidResult();
    if (type === 'Direct') {
      const payment = readStruct(args[0], ['address', 'memo']);
      const address = readAddress(payment.address);
      const memo = readMemo(payment.memo);
      if (StrKey.isValidContract(address) && memo.type !== 'none')
        invalidResult();
      return { address, memo };
    }
    if (type === 'Muxed') {
      const payment = readStruct(args[0], ['account', 'id']);
      const account = readAddress(payment.account, true);
      const id = readId(payment.id);
      return {
        address: new MuxedAccount(new Account(account, '0'), id).accountId(),
        memo: { type: 'none' },
      };
    }
    return invalidResult();
  } catch (error) {
    if (error instanceof SoranReadError) throw error;
    return invalidResult();
  }
}

function contractError(code?: number): SoranReadError {
  // Lookup 5 means an unallocated namespace; 7 means an inactive name (including
  // suspended children). This is a search miss, not proof of availability.
  if (code === 5)
    return new SoranReadError(
      'Soran namespace was not found on Testnet.',
      'not_found'
    );
  if (code === 7)
    return new SoranReadError(
      'This Soran name has no active destination on Testnet.',
      'not_found'
    );
  return new SoranReadError(
    'Soran could not resolve this name. Try again later.'
  );
}

type ContractRead = (method: string, args?: xdr.ScVal[]) => Promise<xdr.ScVal>;

/** SDK transport and unsigned simulation only. One deadline covers all reads.
 * Lookup evaluates routing, active ownership/generation and payment metadata.
 * The RPC operator observes names, addresses and IPs; no Soran HTTP API is used. */
async function withLookup<T>(
  signal: AbortSignal | undefined,
  action: (read: ContractRead) => Promise<T>
): Promise<T> {
  const controller = new AbortController();
  const cancel = () => controller.abort(signal?.reason);
  if (signal?.aborted) cancel();
  else signal?.addEventListener('abort', cancel, { once: true });
  const timer = setTimeout(
    () =>
      controller.abort(
        new DOMException('Soran lookup timed out', 'TimeoutError')
      ),
    TIMEOUT_MS
  );
  try {
    controller.signal.throwIfAborted();
    const server = new rpc.Server(RPC_URL);
    // Select SDK's bounded native-fetch adapter. Its per-request timeout would
    // replace fetchOptions.signal, so the shared controller owns the deadline.
    server.httpClient.defaults.timeout = 0;
    server.httpClient.defaults.maxRedirects = 0;
    server.httpClient.defaults.maxContentLength = 1_048_576;
    server.httpClient.defaults.fetchOptions = {
      signal: controller.signal,
      credentials: 'omit',
      cache: 'no-store',
      referrerPolicy: 'no-referrer',
    };
    const contract = new Contract(LOOKUP);
    const read: ContractRead = async (method, args = []) => {
      controller.signal.throwIfAborted();
      const transaction = new TransactionBuilder(
        new Account(SIMULATION_ACCOUNT, '0'),
        {
          fee: '100',
          networkPassphrase: Networks.TESTNET,
        }
      )
        .addOperation(contract.call(method, ...args))
        .setTimeout(30)
        .build();
      // Disable upgraded auth encoding for compatibility with the public RPC.
      const result = await server.simulateTransaction(
        transaction,
        undefined,
        undefined,
        false
      );
      controller.signal.throwIfAborted();
      if (rpc.Api.isSimulationRestore(result)) {
        throw new SoranReadError(
          'Soran contract data needs restoration. Try again later.'
        );
      }
      if (rpc.Api.isSimulationError(result)) {
        // Only classify the top-level Lookup error, never a nested diagnostic.
        const match = /^HostError: Error\(Contract, #(\d+)\)/.exec(
          result.error
        );
        throw contractError(
          method === 'resolve_destination' && match
            ? Number(match[1])
            : undefined
        );
      }
      if (!rpc.Api.isSimulationSuccess(result) || !result.result?.retval) {
        throw new SoranReadError(
          'Soran is temporarily unavailable. Try again.'
        );
      }
      const value = result.result.retval;
      if (value.type === 'scvError') {
        throw contractError(
          method === 'resolve_destination' && value.error.type === 'sceContract'
            ? value.error.contractCode
            : undefined
        );
      }
      return value;
    };
    return await action(read);
  } catch (error) {
    // SDK transport may wrap an abort; preserve the shared deadline's reason.
    if (controller.signal.aborted) throw controller.signal.reason;
    throw error;
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
    controller.abort();
  }
}

function requireVersion(value: xdr.ScVal, expected: number): void {
  if (value.type !== 'scvU32' || value.u32 !== expected) {
    throw new SoranReadError(
      'This Soran deployment is incompatible with the explorer.'
    );
  }
}

async function checkLookup(read: ContractRead, subname = false): Promise<void> {
  const [registry, version, destinationVersion, subnameVersion] =
    await Promise.all([
      read('registry'),
      read('version'),
      read('destination_version'),
      ...(subname ? [read('subname_version')] : []),
    ]);
  if (readAddress(registry) !== REGISTRY) {
    throw new SoranReadError(
      'This Soran deployment is incompatible with the explorer.'
    );
  }
  requireVersion(version, 2);
  requireVersion(destinationVersion, 2);
  if (subname) requireVersion(subnameVersion, 1);
}

export async function resolveSoranPayment(
  name: string,
  network: Network,
  signal?: AbortSignal
): Promise<SoranResolve> {
  if (network !== 'testnet')
    return {
      kind: 'failed',
      reason: 'Soran names are currently available on Testnet only.',
    };
  const parsed = parseSoranName(name);
  if (!parsed) return { kind: 'failed', reason: 'Enter a valid Soran name.' };
  try {
    const payment = await withLookup(signal, async (read) => {
      await checkLookup(read, parsed.label.includes('.'));
      return decodeSoranDestination(
        await read('resolve_destination', [
          nativeToScVal(parsed.name, { type: 'string' }),
        ])
      );
    });
    return { kind: 'resolved', payment };
  } catch (error) {
    if (
      !signal?.aborted &&
      error instanceof SoranReadError &&
      error.code === 'not_found'
    ) {
      return { kind: 'failed', reason: error.message, code: error.code };
    }
    return {
      kind: 'failed',
      reason: signal?.aborted
        ? 'Soran lookup was cancelled.'
        : error instanceof DOMException && error.name === 'TimeoutError'
        ? 'Soran lookup timed out. Try again.'
        : error instanceof SoranReadError
        ? error.message
        : 'Soran is temporarily unavailable. Try again.',
    };
  }
}

/** Lookup's primary methods verify the elected name against the current
 * resolver. A full M identity uses its base account AND u64 ID, not a G hint.
 * No holder/reverse hint or hosted-API fallback is allowed. */
export async function resolveSoranPrimary(
  address: string,
  network: Network,
  signal?: AbortSignal
): Promise<string | null> {
  if (
    network !== 'testnet' ||
    !(
      StrKey.isValidEd25519PublicKey(address) ||
      StrKey.isValidContract(address) ||
      StrKey.isValidMed25519PublicKey(address)
    )
  )
    return null;
  try {
    return await withLookup(signal, async (read) => {
      await checkLookup(read);
      let value: xdr.ScVal;
      if (StrKey.isValidMed25519PublicKey(address)) {
        requireVersion(await read('muxed_identity_version'), 1);
        const muxed = MuxedAccount.fromAddress(address, '0');
        value = await read('primary_name_muxed', [
          new Address(muxed.baseAccount().accountId()).toScVal(),
          nativeToScVal(BigInt(muxed.id()), { type: 'u64' }),
        ]);
      } else {
        if (readAddress(await read('primary')) !== PRIMARY) {
          throw new SoranReadError(
            'This Soran Primary deployment is incompatible with the explorer.'
          );
        }
        value = await read('primary_name', [new Address(address).toScVal()]);
      }
      if (value.type === 'scvVoid') return null;
      if (value.type !== 'scvString') invalidResult();
      const name = value.str.toStringStrict();
      const parsed = parseSoranName(name);
      if (!parsed || parsed.name !== name) invalidResult();
      if (parsed.label.includes('.'))
        requireVersion(await read('subname_version'), 1);
      return name;
    });
  } catch {
    // Optional display information: retain the original address on any failure.
    return null;
  }
}
