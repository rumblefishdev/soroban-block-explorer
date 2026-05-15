import { useParams } from 'react-router-dom';

import { PageStub } from './PageStub.js';

export default function AssetDetailPage() {
  const { id } = useParams<{ id: string }>();
  return (
    <PageStub
      title="Asset"
      path="/assets/:id"
      rows={[{ label: 'id', value: id }]}
    />
  );
}
