import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';
import { TestnetBadge } from '../TestnetBadge.js';

describe('TestnetBadge', () => {
  it('marks a testnet page', () => {
    renderWithProviders(<TestnetBadge current="testnet" />);
    expect(screen.getByText('TESTNET')).toBeInTheDocument();
  });

  it('shows nothing on mainnet', () => {
    renderWithProviders(<TestnetBadge current="mainnet" />);
    expect(screen.queryByText('TESTNET')).toBeNull();
  });
});
