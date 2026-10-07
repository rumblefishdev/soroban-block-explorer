import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';
import { ContractInterface } from '../ContractInterface.js';

import type { InterfaceResponse } from '@rumblefish/api-types';

let response: InterfaceResponse;

vi.mock('../../../api/index.js', () => ({
  useContractInterface: () => ({
    data: response,
    isLoading: false,
    isError: false,
    error: null,
    refetch: vi.fn(),
  }),
}));

// A real program without a `contractspecv0` section on mainnet.
const CONTRACT = 'CA3M5DXSSDUSTAXIPH5GGTS55SIZJKUHKQEOJVZ57GMZKEQCGZBTY7ZV';
const WASM_HASH =
  'cd8ad034fd37e246ca578b1cfd1dc3d95fb532a6591ecf491ee01d2db03f5244';

describe('Interface tab with no function list', () => {
  it('points a WASM without interface metadata to the Code tab', () => {
    response = {
      contract_id: CONTRACT,
      wasm_hash: WASM_HASH,
      interface_metadata: null,
    };
    const onShowCode = vi.fn();
    renderWithProviders(
      <ContractInterface contractId={CONTRACT} onShowCode={onShowCode} />
    );

    expect(screen.getByText('No interface metadata')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Open Code' }));
    expect(onShowCode).toHaveBeenCalledOnce();
  });

  it('treats a declared but empty function list the same way', () => {
    response = {
      contract_id: CONTRACT,
      wasm_hash: WASM_HASH,
      interface_metadata: { functions: [], wasm_byte_len: 1074 },
    };
    renderWithProviders(
      <ContractInterface contractId={CONTRACT} onShowCode={vi.fn()} />
    );

    expect(screen.getByText('No interface metadata')).toBeInTheDocument();
  });

  it('keeps the SAC / pre-upload message when there is no WASM', () => {
    response = { contract_id: CONTRACT, wasm_hash: null };
    renderWithProviders(
      <ContractInterface contractId={CONTRACT} onShowCode={vi.fn()} />
    );

    expect(screen.getByText('No public interface')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Open Code' })).toBeNull();
  });

  it('shows no button when the page has no Code tab to open', () => {
    response = {
      contract_id: CONTRACT,
      wasm_hash: WASM_HASH,
      interface_metadata: null,
    };
    renderWithProviders(<ContractInterface contractId={CONTRACT} />);

    expect(screen.getByText('No interface metadata')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Open Code' })).toBeNull();
  });
});
