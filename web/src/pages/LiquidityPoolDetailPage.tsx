import { useParams } from 'react-router-dom';

import { PageStub } from './PageStub.js';

export default function LiquidityPoolDetailPage() {
  const { id } = useParams<{ id: string }>();
  return (
    <PageStub
      title="Liquidity pool"
      path="/liquidity-pools/:id"
      rows={[{ label: 'id', value: id }]}
    />
  );
}
