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
