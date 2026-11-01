import { useContext, type MouseEvent, type RefObject } from 'react';

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
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import type { LargeTransferDetail } from '@/ipc/types';
import { formatDecimalSize } from '@/lib/format';

interface Props {
  /** The decided plan's numbers, as the backend refused them. */
  detail: LargeTransferDetail;
  /** Where focus goes when the dialog closes (the apply button). */
  returnFocus?: RefObject<HTMLElement | null>;
  onCancel(): void;
  onConfirm(bytes: number): void;
}

/**
 * The confirmation before Organizar writes 10 GB or more (systems batch 3
 * D23, §9.1): the question carries the exact size, the body names the
 * drive and its free space, and the button repeats the size. Mono only
 * for sizes; focus starts on «Cancelar»; Escape cancels.
 */
export function LargeTransferDialog({
  detail,
  returnFocus,
  onCancel,
  onConfirm,
}: Props) {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const bytes = formatDecimalSize(detail.bytes, locale);
  const one = detail.items === 1;
  const body =
    detail.free === null
      ? one
        ? 'organize.largeTransfer.bodyNoFreeOne'
        : 'organize.largeTransfer.bodyNoFree'
      : one
        ? 'organize.largeTransfer.bodyOne'
        : 'organize.largeTransfer.body';
  const confirm = (e: MouseEvent) => {
    // The parent closes it once the apply is under way: Radix must not
    // report this click as a dismissal (that would cancel).
    e.preventDefault();
    onConfirm(detail.bytes);
  };
  return (
    <AlertDialog
      open
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <AlertDialogContent
        onCloseAutoFocus={(e) => {
          const target = returnFocus?.current;
          if (!target) return;
          e.preventDefault();
          target.focus();
        }}
      >
        <AlertDialogHeader>
          <AlertDialogTitle>
            {tr(
              detail.copies
                ? 'organize.largeTransfer.title'
                : 'organize.largeTransfer.titleMove',
              { bytes },
            )}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {tr(
              body,
              {
                count: detail.items,
                volume: detail.volume,
                free:
                  detail.free === null
                    ? ''
                    : formatDecimalSize(detail.free, locale),
              },
              ['free'],
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>
            {t('organize.largeTransfer.cancel')}
          </AlertDialogCancel>
          <AlertDialogAction onClick={confirm}>
            {tr(
              detail.copies
                ? 'organize.largeTransfer.confirm'
                : 'organize.largeTransfer.confirmMove',
              { bytes },
            )}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
