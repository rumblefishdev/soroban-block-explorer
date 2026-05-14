import { useParams } from 'react-router-dom';

import { PageStub } from './PageStub.js';

export default function ContractDetailPage() {
  const { contractId } = useParams<{ contractId: string }>();
  return (
    <PageStub
      title="Contract"
      path="/contracts/:contractId"
      rows={[{ label: 'contractId', value: contractId }]}
    />
  );
}
