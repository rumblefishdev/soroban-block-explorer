import { screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../test-utils.js';

import { PoolParticipants } from './PoolParticipants.js';

const hookMock = vi.hoisted(() => ({ usePoolParticipants: vi.fn() }));

vi.mock('../../api/index.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../api/index.js')>()),
  usePoolParticipants: hookMock.usePoolParticipants,
}));

describe('PoolParticipants empty state', () => {
  beforeEach(() => {
    hookMock.usePoolParticipants.mockReturnValue({
      data: { data: [], page: { limit: 20 } },
      isLoading: false,
      isPlaceholderData: false,
      isError: false,
      error: null,
      refetch: vi.fn(),
    });
  });

  // Each of the three counts is a different fact, and only one of them is
  // "nobody provides liquidity here".
  it('says none only when the count is a measured zero', () => {
    renderWithProviders(<PoolParticipants poolId="C1" knownParticipants={0} />);
    expect(screen.getByText('No participants yet')).toBeInTheDocument();
  });

  it('says counted but not listed when providers exist', () => {
    renderWithProviders(
      <PoolParticipants poolId="C1" knownParticipants={136} />
    );
    expect(screen.getByText('Participants not listed')).toBeInTheDocument();
  });

  it('says not indexed when the count itself is unknown', () => {
    renderWithProviders(
      <PoolParticipants poolId="C1" knownParticipants={null} />
    );
    expect(screen.getByText('Participants not indexed')).toBeInTheDocument();
    expect(screen.queryByText('No participants yet')).not.toBeInTheDocument();
  });
});
