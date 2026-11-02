import {
  useContext,
  useEffect,
  useId,
  useRef,
  type MouseEvent,
  type RefObject,
} from 'react';

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import type { LargeTransferDetail } from '@/ipc/types';
import { formatDecimalSize } from '@/lib/format';
import { roomForItem } from '@/lib/largeTransfer';
import type { HugeAnswer } from '@/stores/organizeStore';

interface Props {
  /** The decided plan's numbers, as the backend refused them. */
  detail: LargeTransferDetail;
  /** The huge item being asked (index into `detail.huge`). */
  cursor: number;
  /** Ids of the items accepted so far in this sequence. */
  accepted: readonly number[];
  /** Where focus goes when the sequence ends (the apply button). */
  returnFocus?: RefObject<HTMLElement | null>;
  onAnswer(answer: HugeAnswer): void;
}

/**
 * The question for each item that writes 10 GB or more on its own, or
 * extracts a game folder out of an archive (adjustments D15–D17, spec
 * §9.3). One dialog for the whole sequence: answering swaps its content
 * in place, so focus and the screen reader's context stay. The body says
 * what the item takes and what will be left free on the target after the
 * rest and the items already accepted (1 % margin, as apply checks); an
 * item that does not fit takes only «No». Focus starts on «No» for every
 * item; Escape is «No».
 */
export function HugeItemDialog({
  detail,
  cursor,
  accepted,
  returnFocus,
  onAnswer,
}: Props) {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const no = useRef<HTMLButtonElement>(null);
  const reasonId = useId();
  const item = detail.huge[cursor];
  // Every new item starts on the safe answer, like the first one.
  useEffect(() => {
    no.current?.focus();
  }, [cursor]);
  if (!item) return null;
  const size = (n: number) => formatDecimalSize(n, locale);
  const room = roomForItem(detail, accepted, item);
  const fits = room?.fits !== false;
  const title = item.extract
    ? 'organize.huge.titleExtract'
    : detail.copies
      ? 'organize.huge.titleCopy'
      : 'organize.huge.titleMove';
  const answer = (a: HugeAnswer) => (e: MouseEvent) => {
    // The dialog stays open across items and the parent closes it at the
    // end: Radix must not take any of these clicks as a dismissal.
    e.preventDefault();
    if (a !== 'no' && !fits) return;
    onAnswer(a);
  };
  // Still focusable and announced with the reason (spec §9.3), drawn
  // like a disabled button.
  const blocked = fits
    ? {}
    : {
        'aria-disabled': true as const,
        'aria-describedby': reasonId,
        className:
          'bg-cr-surface-sunken text-cr-text-disabled shadow-none hover:bg-cr-surface-sunken active:bg-cr-surface-sunken',
      };
  return (
    <AlertDialog open>
      <AlertDialogContent
        size="md"
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          no.current?.focus();
        }}
        onEscapeKeyDown={(e) => {
          e.preventDefault();
          onAnswer('no');
        }}
        onCloseAutoFocus={(e) => {
          const target = returnFocus?.current;
          if (!target) return;
          e.preventDefault();
          target.focus();
        }}
      >
        <AlertDialogHeader>
          <AlertDialogTitle className="break-words">
            {t(title, { name: item.name })}
          </AlertDialogTitle>
          <AlertDialogDescription asChild>
            <div className="grid gap-1.5">
              {room === null ? (
                <p className="m-0">
                  {tr('organize.huge.bodyNoFree', { bytes: size(item.bytes) })}
                </p>
              ) : room.fits ? (
                <p className="m-0">
                  {tr(
                    'organize.huge.body',
                    {
                      bytes: size(item.bytes),
                      free: size(room.left),
                      volume: detail.volume,
                    },
                    ['bytes', 'free'],
                  )}
                </p>
              ) : (
                <p id={reasonId} className="m-0 text-cr-text">
                  {tr(
                    'organize.huge.noRoom',
                    { volume: detail.volume, missing: size(room.missing) },
                    ['missing'],
                  )}
                </p>
              )}
              <p className="m-0 text-caption">
                {tr('organize.huge.progress', {
                  index: cursor + 1,
                  count: detail.huge.length,
                })}
              </p>
            </div>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel ref={no} onClick={answer('no')}>
            {t('organize.huge.no')}
          </AlertDialogCancel>
          <Button variant="secondary" onClick={answer('all')} {...blocked}>
            {t('organize.huge.all')}
          </Button>
          <AlertDialogAction onClick={answer('yes')} {...blocked}>
            {t('organize.huge.yes')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
