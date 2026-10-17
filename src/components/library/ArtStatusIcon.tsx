import { SlotMark, type SlotState } from '@/components/ui/slot-mark';
import { useT } from '@/i18n/useT';
import type { ArtMark, Media } from '@/ipc/types';

import { Tip } from './Tip';

/** The brand slot: solid = present, dashed = missing, struck = the layout
 * cannot store it. */
const STATE: Record<ArtMark, SlotState> = {
  present: 'filled',
  missing: 'missing',
  unsupported: 'unsupported',
};

export function ArtStatusIcon({
  media,
  mark,
}: {
  media: Media;
  mark: ArtMark;
}) {
  const t = useT();
  const label = t('art.status', {
    media: t(`art.media.${media}`),
    state: t(`art.mark.${mark}`),
  });
  return (
    // The span takes the trigger's data-state, so the slot keeps its own.
    <Tip content={label}>
      <span className="inline-flex" data-mark={mark}>
        <SlotMark state={STATE[mark]} size={12} label={label} />
      </span>
    </Tip>
  );
}
