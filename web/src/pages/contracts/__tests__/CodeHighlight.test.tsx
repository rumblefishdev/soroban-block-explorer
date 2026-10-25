import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';
import CodeHighlight from '../CodeHighlight.js';

const NBSP = String.fromCharCode(0xa0);

describe('CodeHighlight', () => {
  it('keeps a no-break space through highlighting', () => {
    const { container } = renderWithProviders(
      <CodeHighlight source={`let a${NBSP}= 1;`} language="rust" />
    );
    expect(container.querySelector('pre')?.textContent).toBe(
      `let a${NBSP}= 1;`
    );
  });

  it('numbers lines without one for a final newline', () => {
    const { container } = renderWithProviders(
      <CodeHighlight source={'a\nb\n'} language="none" />
    );
    const gutter = container.querySelector('[aria-hidden]');
    expect(gutter?.textContent).toBe('12');
  });

  it('draws a lone carriage return as a marked line break', () => {
    const { container } = renderWithProviders(
      <CodeHighlight source={'a\rb'} language="none" />
    );
    expect(container.querySelector('pre')?.textContent).toBe('a␍\nb');
  });
});
