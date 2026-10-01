import HourglassIcon from '@mui/icons-material/HourglassEmptyOutlined';
import { EmptyState } from '@rumblefish/soroban-block-explorer-ui';

import { SectionCard } from '../detail/SectionCard.js';

interface NotIndexedSectionProps {
  title: string;
  what: string;
}

/**
 * Stands in for a pool-detail section whose data the indexer does not record
 * for this pool kind yet. An empty table there would read as "nothing
 * happened"; this says the truth — nothing was read.
 */
export function NotIndexedSection({ title, what }: NotIndexedSectionProps) {
  return (
    <SectionCard title={title}>
      <EmptyState
        icon={<HourglassIcon />}
        title="Not indexed yet"
        description={`${what} for Soroban pools is not indexed yet.`}
      />
    </SectionCard>
  );
}
