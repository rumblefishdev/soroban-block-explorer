import { render, screen } from '@testing-library/react';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { describe, expect, it } from 'vitest';

import { EventsSection } from './EventsSection.js';
import { RawDataSection } from './RawDataSection.js';

function wrap(node: React.ReactNode) {
  return render(<ExplorerThemeProvider>{node}</ExplorerThemeProvider>);
}

/**
 * Both sections read fields that exist only in the archive-fetched block, so an
 * empty render has two indistinguishable causes. They must never state a count
 * they did not measure (0377 F2).
 */
describe('archive-gated sections', () => {
  it('EventsSection reports unavailable instead of "0 events"', () => {
    wrap(
      <EventsSection contractEvents={[]} diagnosticEvents={[]} unavailable />
    );

    expect(screen.getByText('Events unavailable')).toBeInTheDocument();
    expect(screen.queryByText('No events emitted.')).toBeNull();
    expect(screen.queryByText('0 events')).toBeNull();
  });

  it('EventsSection still reports a measured zero when the data did load', () => {
    wrap(
      <EventsSection
        contractEvents={[]}
        diagnosticEvents={[]}
        unavailable={false}
      />
    );

    expect(screen.getByText('No events emitted.')).toBeInTheDocument();
    expect(screen.queryByText('Events unavailable')).toBeNull();
  });

  it('RawDataSection reports unavailable instead of "0 sections"', () => {
    wrap(
      <RawDataSection
        envelopeXdr={null}
        resultXdr={null}
        resultMetaXdr={null}
        unavailable
      />
    );

    expect(screen.getByText('Raw data unavailable')).toBeInTheDocument();
    expect(
      screen.queryByText('No raw XDR available for this transaction.')
    ).toBeNull();
  });
});
