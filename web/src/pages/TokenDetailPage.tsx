import { useParams } from 'react-router-dom';

import { PageStub } from './PageStub.js';

export default function TokenDetailPage() {
  const { id } = useParams<{ id: string }>();
  return (
    <PageStub
      title="Token"
      path="/tokens/:id"
      rows={[{ label: 'id', value: id }]}
    />
  );
}
