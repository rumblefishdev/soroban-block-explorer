/**
 * Returns the URL only when it is a safe `http(s)` link, otherwise `null`.
 *
 * Off-chain metadata (asset SEP-1 TOML) is attacker-controlled, so a
 * `javascript:` — or any other-scheme — value must never reach an `href`
 * or an image `src`.
 */
export function safeHttpUrl(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    const { protocol } = new URL(url);
    return protocol === 'http:' || protocol === 'https:' ? url : null;
  } catch {
    return null;
  }
}
