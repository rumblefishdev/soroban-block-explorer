import { Card } from '@mui/material';

import { DataList, type DataListProps } from './DataList.js';

/**
 * A list page's table section: `DataList` (filters + body + pager) in a plain
 * `Card`. Detail sections with their own frame render `DataList` directly.
 */
export function DataListCard<T>(props: DataListProps<T>) {
  return (
    <Card>
      <DataList<T> {...props} />
    </Card>
  );
}
