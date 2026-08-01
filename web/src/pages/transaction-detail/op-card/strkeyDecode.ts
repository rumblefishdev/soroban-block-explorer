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

/** Version byte expected for each leading letter we link. */
const VERSION_BY_PREFIX: Record<string, number> = {
  C: STRKEY_VERSION.contract,
  G: STRKEY_VERSION.account,
  L: STRKEY_VERSION.pool,
};

const STRKEY_SHAPE = /^[GCL][A-Z2-7]{55}$/;

/** True when `value` is a strkey whose CHECKSUM verifies — not merely one
 *  that looks like one.
 *
 *  The shape alone is 56 base32 characters, which any long base32 blob
 *  satisfies: contract event data and memos can carry such strings, and
 *  linking one produces a confident link to a page that cannot exist. The
 *  CRC16 makes the claim provable, so only a real address is rendered as an
 *  address. */
export function isValidStrkey(value: string): boolean {
  if (!STRKEY_SHAPE.test(value)) return false;
  const bytes = new Uint8Array(35);
  let buffer = 0;
  let bits = 0;
  let out = 0;
  for (const char of value) {
    const index = BASE32_ALPHABET.indexOf(char);
    if (index < 0) return false;
    buffer = (buffer << 5) | index;
    bits += 5;
    if (bits >= 8) {
      bytes[out++] = (buffer >> (bits - 8)) & 0xff;
      bits -= 8;
    }
  }
  // 56 base32 chars carry 280 bits; the 35-byte payload uses 280, so the
  // decode must land exactly.
  if (out !== 35) return false;
  if (bytes[0] !== VERSION_BY_PREFIX[value[0]]) return false;
  const crc = crc16xmodem(bytes.subarray(0, 33));
  return bytes[33] === (crc & 0xff) && bytes[34] === crc >> 8;
}

/** Encode 32 raw bytes (base64) as a strkey with the given version byte.
 *  Total: ANY 32 bytes yield a checksum-valid strkey — a successful
 *  decode proves nothing by itself, so never decode blindly. */
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
