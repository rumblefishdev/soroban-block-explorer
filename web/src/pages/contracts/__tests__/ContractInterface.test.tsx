import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';
import { ContractInterface } from '../ContractInterface.js';

vi.mock('../../../api/index.js', () => ({
  useContractInterface: () => ({
    data: { interface_metadata: null },
    isLoading: false,
    isError: false,
    error: null,
    refetch: vi.fn(),
  }),
}));

// A real program without a `contractspecv0` section on mainnet.
const CONTRACT = 'CA3M5DXSSDUSTAXIPH5GGTS55SIZJKUHKQEOJVZ57GMZKEQCGZBTY7ZV';

describe('Interface tab with no interface metadata', () => {
  it('says the program carries no description and opens the Code tab', () => {
    const onShowCode = vi.fn();
    renderWithProviders(
      <ContractInterface
        contractId={CONTRACT}
        hasWasm={true}
        onShowCode={onShowCode}
      />
    );

    expect(screen.getByText('No interface description')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Open Code' }));
    expect(onShowCode).toHaveBeenCalledOnce();
  });

  it('keeps the SAC / pre-upload message when there is no program', () => {
    renderWithProviders(
      <ContractInterface
        contractId={CONTRACT}
        hasWasm={false}
        onShowCode={vi.fn()}
      />
    );

    expect(screen.getByText('No public interface')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Open Code' })).toBeNull();
  });
});
