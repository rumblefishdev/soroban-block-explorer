import {
  encodeStrkey,
  STRKEY_VERSION,
} from '@rumblefish/soroban-block-explorer-ui';

export { STRKEY_VERSION };

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
  const bytes = new Uint8Array(32);
  for (let i = 0; i < 32; i++) bytes[i] = bin.charCodeAt(i);
  return encodeStrkey(bytes, version);
}
