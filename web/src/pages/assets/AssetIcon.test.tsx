import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';

import { AssetIcon } from './AssetIcon.js';

describe('AssetIcon', () => {
  it('falls back to the code initial when there is no icon url', () => {
    const { container, getByText } = renderWithProviders(
      <AssetIcon code="USDC" />
    );
    expect(container.querySelector('img')).toBeNull();
    expect(getByText('U')).toBeInTheDocument();
  });

  it('renders the sanitised http icon url', () => {
    const url = 'https://cdn.example.test/icons/usdc.svg';
    const { container } = renderWithProviders(
      <AssetIcon code="USDC" iconUrl={url} />
    );
    expect(container.querySelector('img')?.getAttribute('src')).toBe(url);
  });

  it('drops an unsafe (non-http) icon url and shows the letter', () => {
    // safeHttpUrl rejects javascript:/data: etc — no <img> is emitted.
    const { container, getByText } = renderWithProviders(
      <AssetIcon code="USDC" iconUrl="javascript:alert(1)" />
    );
    expect(container.querySelector('img')).toBeNull();
    expect(getByText('U')).toBeInTheDocument();
  });
});
