import HourglassIcon from '@mui/icons-material/HourglassEmptyOutlined';
import { EmptyState } from '@rumblefish/soroban-block-explorer-ui';

import { SectionCard } from '../detail/SectionCard.js';

interface NotIndexedSectionProps {
  title: string;
  /** What is missing, in the reader's words ("its trades", "its holders"). */
  what: string;
}

/**
 * A detail section whose data the explorer does not index yet for this pool.
 * Says so instead of rendering the section's own empty state, which would
 * claim the pool HAS no activity or participants (task 0374).
 */
export function NotIndexedSection({ title, what }: NotIndexedSectionProps) {
  return (
    <SectionCard title={title}>
      <EmptyState
        icon={<HourglassIcon />}
        title="Not indexed yet"
        description={`The explorer does not record ${what} for Soroban pools yet, so this section cannot show them.`}
      />
    </SectionCard>
  );
}
