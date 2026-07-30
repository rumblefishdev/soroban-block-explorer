import { createContext, useContext } from 'react';

/** SEP-23 strkey encoding: version byte + 32-byte payload + CRC16-XModem
 *  (little-endian), base32 without padding. Version bytes are the base32
 *  index of the leading letter shifted left by 3 — G (account) = 6<<3,
 *  C (contract) = 2<<3, L (liquidity pool) = 11<<3. */
const BASE32_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

export const STRKEY_VERSION = {
  contract: 2 << 3,
  account: 6 << 3,
  pool: 11 << 3,
} as const;

function crc16xmodem(bytes: Uint8Array): number {
  let crc = 0;
  for (const byte of bytes) {
    crc ^= byte << 8;
    for (let bit = 0; bit < 8; bit++) {
      crc = crc & 0x8000 ? ((crc << 1) ^ 0x1021) & 0xffff : (crc << 1) & 0xffff;
    }
  }
  return crc;
}

function base32(bytes: Uint8Array): string {
  let out = '';
  let buffer = 0;
  let bits = 0;
  for (const byte of bytes) {
    buffer = (buffer << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      out += BASE32_ALPHABET[(buffer >> (bits - 5)) & 31];
      bits -= 5;
    }
  }
  if (bits > 0) out += BASE32_ALPHABET[(buffer << (5 - bits)) & 31];
  return out;
}

/** Encode 32 raw bytes (base64) as a strkey with the given version byte.
 *  Total: ANY 32 bytes yield a checksum-valid strkey, so a successful
 *  decode proves nothing by itself — see `corroboratedStrkey`. */
export function strkeyFromBase64(b64: string, version: number): string | null {
  let bin: string;
  try {
    bin = atob(b64);
  } catch {
    return null;
  }
  if (bin.length !== 32) return null;
  const payload = new Uint8Array(35);
  payload[0] = version;
  for (let i = 0; i < 32; i++) payload[i + 1] = bin.charCodeAt(i);
  const crc = crc16xmodem(payload.subarray(0, 33));
  payload[33] = crc & 0xff;
  payload[34] = crc >> 8;
  return base32(payload);
}

const STRKEY_STRING_RE = /^[GCL][A-Z2-7]{55}$/;

/** Every strkey that literally occurs anywhere in the transaction payload —
 *  the corroboration set for decoding raw bytes. */
export function collectStrkeys(value: unknown, into = new Set<string>()) {
  if (typeof value === 'string') {
    if (STRKEY_STRING_RE.test(value)) into.add(value);
  } else if (Array.isArray(value)) {
    for (const item of value) collectStrkeys(item, into);
  } else if (value != null && typeof value === 'object') {
    for (const item of Object.values(value)) collectStrkeys(item, into);
  }
  return into;
}

/** Decode 32 raw bytes as C/G/L candidates and return the first one that
 *  ALREADY OCCURS elsewhere in the same transaction. Any 32 bytes decode to
 *  a checksum-valid strkey, so blind decoding would fabricate addresses out
 *  of arbitrary hashes — the in-transaction occurrence is what makes the
 *  reading safe (zero false links by construction). */
export function corroboratedStrkey(
  b64: string,
  knownIds: ReadonlySet<string>
): string | null {
  if (knownIds.size === 0) return null;
  for (const version of Object.values(STRKEY_VERSION)) {
    const candidate = strkeyFromBase64(b64, version);
    if (candidate != null && knownIds.has(candidate)) return candidate;
  }
  return null;
}

/** Strkeys occurring in the currently viewed transaction; provided by the
 *  transaction-detail page, consumed by the JSON viewer's bytes hint. */
export const TxKnownIdsContext = createContext<ReadonlySet<string>>(new Set());

export function useTxKnownIds(): ReadonlySet<string> {
  return useContext(TxKnownIdsContext);
}
