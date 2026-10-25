/** SEP-23 strkey encoding: version byte + payload + CRC16-XModem
 *  (little-endian), base32 without padding. Version bytes are the base32
 *  index of the leading letter shifted left by 3 — G (account) = 6<<3,
 *  C (contract) = 2<<3, L (liquidity pool) = 11<<3, M (muxed account) =
 *  12<<3, W (muxed contract, CAP-0084) = 22<<3. */
const BASE32_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

export const STRKEY_VERSION = {
  contract: 2 << 3,
  account: 6 << 3,
  pool: 11 << 3,
  muxedAccount: 12 << 3,
  muxedContract: 22 << 3,
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

/** A strkey for 32 raw bytes with the given version byte. */
export function encodeStrkey(bytes32: Uint8Array, version: number): string {
  const payload = new Uint8Array(35);
  payload[0] = version;
  payload.set(bytes32, 1);
  const crc = crc16xmodem(payload.subarray(0, 33));
  payload[33] = crc & 0xff;
  payload[34] = crc >> 8;
  return base32(payload);
}

/** The address a muxed strkey stands for: an `M…` (an account with a
 *  sub-account id) → its `G…`, a `W…` (a contract with a sub-account id,
 *  CAP-0084) → its `C…`. Both carry the 32-byte key + an 8-byte id.
 *  `null` unless the whole key decodes canonically and its checksum holds —
 *  the same keys the API accepts. */
export function unmuxed(value: string): string | null {
  // 1 version byte + 40 payload bytes + 2 checksum bytes = 43 bytes = 69
  // chars; 69 × 5 = 345 bits, so one trailing bit is padding.
  if (value.length !== 69) return null;
  const bytes = new Uint8Array(43);
  let buffer = 0;
  let bits = 0;
  let n = 0;
  for (const ch of value) {
    const v = BASE32_ALPHABET.indexOf(ch);
    if (v < 0) return null;
    buffer = ((buffer << 5) | v) & 0xffff;
    bits += 5;
    if (bits >= 8) {
      bytes[n++] = (buffer >> (bits - 8)) & 0xff;
      bits -= 8;
    }
  }
  // The padding bit must be zero, or two strings would name one key.
  if ((buffer & ((1 << bits) - 1)) !== 0) return null;
  const crc = crc16xmodem(bytes.subarray(0, 41));
  if (bytes[41] !== (crc & 0xff) || bytes[42] !== crc >> 8) return null;
  const key = bytes.subarray(1, 33);
  if (bytes[0] === STRKEY_VERSION.muxedAccount)
    return encodeStrkey(key, STRKEY_VERSION.account);
  if (bytes[0] === STRKEY_VERSION.muxedContract)
    return encodeStrkey(key, STRKEY_VERSION.contract);
  return null;
}
