import { Plus } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { useT } from '@/i18n/useT';
import type { OrganizeItem } from '@/ipc/types';

/** Whether an item arrived with the last "scan again". */
export type IsNew = (item: OrganizeItem) => boolean;

/** Rows a refresh found: a soft info tint that settles once (globals.css). */
export const newRow = 'organize-new-row';

/** "New" after the name of a file the last scan found. The plus ties it to
 * the "+3" on its group and sets it apart from the confidence badge beside
 * it, which shares the info colour ("Medium"). */
export function NewBadge() {
  const t = useT();
  return (
    <Badge variant="info">
      <Plus aria-hidden />
      {t('organize.refresh.new')}
    </Badge>
  );
}

/** "+3" on a group or bucket header, so a closed one still says where the
 * new files landed. Read as "New: 3". */
export function NewCount({ count }: { count: number }) {
  const t = useT();
  if (count === 0) return null;
  return (
    <Badge variant="info" className="tabular-nums">
      <span aria-hidden>{`+${count}`}</span>
      <span className="sr-only">
        {t('organize.refresh.newCount', { count })}
      </span>
    </Badge>
  );
}
