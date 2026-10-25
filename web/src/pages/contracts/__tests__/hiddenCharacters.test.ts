import { describe, expect, it } from 'vitest';

import { countHidden, displayText, revealHidden } from '../hiddenCharacters.js';

// Built from code points so this file holds no invisible character itself.
const RLO = String.fromCharCode(0x202e);
const ZWSP = String.fromCharCode(0x200b);
const BOM = String.fromCharCode(0xfeff);

describe('hidden characters', () => {
  it('writes out direction controls and zero-width characters', () => {
    const text = `if level != "user${RLO} // admin"`;
    expect(revealHidden(text)).toBe('if level != "user⟨U+202E⟩ // admin"');
    expect(revealHidden(`--lo${ZWSP}cked`)).toBe('--lo⟨U+200B⟩cked');
    expect(countHidden(`a${RLO}b${ZWSP}c`)).toBe(2);
  });

  it('leaves a leading byte-order mark alone, marks one further in', () => {
    expect(revealHidden(`${BOM}fn main`)).toBe(`${BOM}fn main`);
    expect(countHidden(`${BOM}fn main`)).toBe(0);
    expect(revealHidden(`a${BOM}b`)).toBe('a⟨U+FEFF⟩b');
  });

  it('keeps ordinary text, tabs and CRLF untouched', () => {
    const text = 'fn a() {\r\n\tlet x = 1;\r\n}\n';
    expect(displayText(text)).toBe(text);
    expect(countHidden(text)).toBe(0);
  });

  it('marks and breaks a lone carriage return', () => {
    expect(displayText('a\rb\rc')).toBe('a␍\nb␍\nc');
  });
});
