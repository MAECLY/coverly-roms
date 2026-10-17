import { CircleAlert, CircleCheck, Info, TriangleAlert } from 'lucide-react';
import { Toaster as Sonner, toast, type ToasterProps } from 'sonner';

import { useT } from '@/i18n/useT';

import { Spinner } from './spinner';

export { toast };

/**
 * Transient confirmations ("41 covers applied. Undo"). Mount once near the
 * root. Unstyled Sonner skinned with the tokens, so it follows data-theme.
 * Errors that need reading belong in an Alert, not a toast.
 */
export function Toaster(props: ToasterProps) {
  const t = useT();
  return (
    <Sonner
      position="bottom-right"
      containerAriaLabel={t('ui.notifications')}
      gap={8}
      icons={{
        success: <CircleCheck aria-hidden className="size-4 text-cr-success" />,
        info: <Info aria-hidden className="size-4 text-cr-info" />,
        warning: (
          <TriangleAlert aria-hidden className="size-4 text-cr-warning" />
        ),
        error: <CircleAlert aria-hidden className="size-4 text-cr-danger" />,
        loading: <Spinner size="sm" decorative />,
      }}
      toastOptions={{
        unstyled: true,
        closeButtonAriaLabel: t('ui.dismiss'),
        classNames: {
          toast:
            'group flex w-[356px] items-start gap-2.5 rounded-lg border border-cr-border bg-popover p-3 text-ui text-popover-foreground shadow-overlay',
          title: 'font-medium',
          description: 'text-caption text-cr-text-secondary',
          icon: 'mt-px',
          actionButton:
            'ml-auto h-control-sm shrink-0 rounded-md bg-primary px-2.5 text-ui font-medium text-primary-foreground hover:bg-cr-primary-hover',
          cancelButton:
            'h-control-sm shrink-0 rounded-md px-2.5 text-ui font-medium text-cr-text hover:bg-cr-hover',
          closeButton:
            'absolute -left-1.5 -top-1.5 grid size-5 place-items-center rounded-full border border-cr-border bg-popover text-cr-text-secondary',
        },
      }}
      {...props}
    />
  );
}
