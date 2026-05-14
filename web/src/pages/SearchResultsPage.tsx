import { useSearchParams } from 'react-router-dom';

import { PageStub } from './PageStub.js';

export default function SearchResultsPage() {
  const [params] = useSearchParams();
  const q = params.get('q') ?? '';
  return (
    <PageStub
      title="Search results"
      path="/search?q="
      rows={[{ label: 'q', value: q }]}
    />
  );
}
