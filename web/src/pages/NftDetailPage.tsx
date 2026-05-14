import { useParams } from 'react-router-dom';

import { PageStub } from './PageStub.js';

export default function NftDetailPage() {
  const { id } = useParams<{ id: string }>();
  return (
    <PageStub
      title="NFT"
      path="/nfts/:id"
      rows={[{ label: 'id', value: id }]}
    />
  );
}
