/**
 * Encodes a Stellar Liquidity Pool ID (32-byte hash, transmitted by the
 * backend as a 64-char lowercase hex string) into its SEP-23 `L...`
 * strkey representation — the canonical user-facing format that Stellar
 * Laboratory, stellar.expert, and Horizon all display.
 *
 * Algorithm (SEP-23):
 *   1. Decode the 64-char hex to a 32-byte payload.
 *   2. Prepend the version byte: `11 << 3 = 0x58` (LiquidityPool kind;
 *      the high-5-bits of this byte form the leading `L` after base32).
 *   3. Append the CRC16-XModem checksum (poly 0x1021, init 0x0000, no
 *      reflection) as little-endian 2 bytes.
 *   4. Base32-encode the resulting 35 bytes (RFC 4648 alphabet, no
 *      padding) → 56 ASCII characters.
 *
 * This module is browser-safe: it uses only `Uint8Array` + standard
 * JS — no `Buffer`, no Node-only APIs, no `@stellar/stellar-base`
 * dependency (the encoder is ~60 LOC; pulling the SDK for one
 * function would be 50–100 KB of bundle bloat).
 */

const VERSION_BYTE_LIQUIDITY_POOL = 11 << 3; // 0x58 → leading 'L' after base32
const BASE32_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
const HEX_PATTERN = /^[0-9a-f]+$/;

/** CRC16-XModem (poly 0x1021, init 0x0000, no input/output reflection). */
function crc16Xmodem(data: Uint8Array): number {
  let crc = 0x0000;
  for (const byte of data) {
    crc ^= byte << 8;
    for (let i = 0; i < 8; i++) {
      crc = crc & 0x8000 ? (crc << 1) ^ 0x1021 : crc << 1;
      crc &= 0xffff;
    }
  }
  return crc;
}

/** RFC 4648 base32 encoder, no padding. Input length must be a multiple
 *  of 5 bytes for clean output (35 bytes → 56 chars for strkey). */
function base32Encode(data: Uint8Array): string {
  let bits = 0;
  let value = 0;
  let output = '';
  for (const byte of data) {
    value = (value << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      output += BASE32_ALPHABET[(value >>> (bits - 5)) & 0x1f];
      bits -= 5;
    }
  }
  if (bits > 0) {
    output += BASE32_ALPHABET[(value << (5 - bits)) & 0x1f];
  }
  return output;
}

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

/**
 * Encodes a 64-char lowercase-hex pool ID into a SEP-23 `L...` strkey
 * (56 ASCII chars).
 *
 * Throws if `hex` is not exactly 64 lowercase-hex characters — this
 * function is the single hex→strkey boundary and surfacing bad input
 * loudly is preferable to silently rendering a corrupted strkey.
 */
export function poolIdHexToStrkey(hex: string): string {
  const normalized = hex.trim().toLowerCase();
  if (normalized.length !== 64 || !HEX_PATTERN.test(normalized)) {
    throw new Error(
      `poolIdHexToStrkey: expected 64-char lowercase hex pool id, got "${hex}"`
    );
  }
  const payload = hexToBytes(normalized);
  const versioned = new Uint8Array(33);
  versioned[0] = VERSION_BYTE_LIQUIDITY_POOL;
  versioned.set(payload, 1);
  const checksum = crc16Xmodem(versioned);
  const full = new Uint8Array(35);
  full.set(versioned, 0);
  full[33] = checksum & 0xff; // little-endian (low byte first)
  full[34] = (checksum >>> 8) & 0xff;
  return base32Encode(full);
}
