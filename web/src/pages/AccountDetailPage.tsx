import { useParams } from 'react-router-dom';

import { PageStub } from './PageStub.js';

export default function AccountDetailPage() {
  const { accountId } = useParams<{ accountId: string }>();
  return (
    <PageStub
      title="Account"
      path="/accounts/:accountId"
      rows={[{ label: 'accountId', value: accountId }]}
    />
  );
}
