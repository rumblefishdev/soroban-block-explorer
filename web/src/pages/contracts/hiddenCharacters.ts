/**
 * Characters that change how text looks without showing themselves:
 * zero-width characters, bidirectional controls (the "Trojan Source"
 * family, CVE-2021-42574) and a byte-order mark past the start. Text from a
 * third party is shown with each of them made visible, never silently
 * applied; the copied original stays untouched by the callers.
 */
const HIDDEN = /[\u200B-\u200F\u202A-\u202E\u2060-\u2064\u2066-\u2069\uFEFF]/g;

const codePoint = (c: string) =>
  `⟨U+${(c.codePointAt(0) ?? 0).toString(16).toUpperCase().padStart(4, '0')}⟩`;

/** How many hidden characters the text holds (a leading byte-order mark
 *  is the file's encoding marker, not content, and is not counted). */
export function countHidden(text: string): number {
  const body = text.startsWith('\uFEFF') ? text.slice(1) : text;
  return body.match(HIDDEN)?.length ?? 0;
}

/** The text with every hidden character written out as `⟨U+XXXX⟩`. */
export function revealHidden(text: string): string {
  const bom = text.startsWith('\uFEFF') ? '\uFEFF' : '';
  return bom + text.slice(bom.length).replace(HIDDEN, codePoint);
}

/** Code as drawn: hidden characters written out, and a lone carriage
 *  return (old Mac line ends) marked and broken — CSS would draw it as a
 *  space and join the lines. Copy keeps the original. */
export function displayText(source: string): string {
  return revealHidden(source).replace(/\r(?!\n)/g, '␍\n');
}
