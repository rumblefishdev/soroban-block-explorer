import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';
import { NftMediaPreview } from '../NftMediaPreview.js';

// Shape of the Stroopies art (CDC5ZX…JFGB): an SVG inlined as a data: URL.
const INLINE_SVG =
  "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'><path stroke-opacity='.06' stroke-dasharray='.1 .4'/></svg>";

describe('NFT media preview (task 0654)', () => {
  it('shows an inline data:image as an image', () => {
    renderWithProviders(
      <NftMediaPreview mediaUrl={INLINE_SVG} name="Stroopies 0" />
    );
    expect(screen.getByRole('img', { name: 'Stroopies 0' })).toHaveAttribute(
      'src',
      INLINE_SVG
    );
  });

  it('still refuses a link with an unknown extension', () => {
    renderWithProviders(
      <NftMediaPreview mediaUrl="https://example.com/a.json" name="X" />
    );
    expect(screen.getByText('Failed to load media')).toBeInTheDocument();
  });
});
