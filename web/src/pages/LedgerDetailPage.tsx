import { useParams } from 'react-router-dom';

import { PageStub } from './PageStub.js';

export default function LedgerDetailPage() {
  const { sequence } = useParams<{ sequence: string }>();
  return (
    <PageStub
      title="Ledger"
      path="/ledgers/:sequence"
      rows={[{ label: 'sequence', value: sequence }]}
    />
  );
}
