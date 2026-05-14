import { useParams } from 'react-router-dom';

import { PageStub } from './PageStub.js';

export default function TransactionDetailPage() {
  const { hash } = useParams<{ hash: string }>();
  return (
    <PageStub
      title="Transaction"
      path="/transactions/:hash"
      rows={[{ label: 'hash', value: hash }]}
    />
  );
}
