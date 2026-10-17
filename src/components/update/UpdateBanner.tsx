import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { useT } from '@/i18n/useT';
import { cn } from '@/lib/utils';
import { useNavStore } from '@/stores/navStore';
import { useUpdateStore } from '@/stores/updateStore';

/** One line when the startup check found an update; never for errors. */
export function UpdateBanner({ className }: { className?: string }) {
  const t = useT();
  const show = useUpdateStore(
    (s) =>
      s.status === 'available' && s.trigger === 'startup' && !s.bannerDismissed,
  );
  const version = useUpdateStore((s) => s.offer?.version ?? '');
  const dismiss = useUpdateStore((s) => s.dismissBanner);
  if (!show) return null;
  return (
    <Alert
      variant="info"
      className={cn('mx-6 mt-3 items-center text-left', className)}
      action={
        <>
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              dismiss();
              useNavStore.getState().openSettings('about');
            }}
          >
            {t('update.bannerDetails')}
          </Button>
          <Button variant="ghost" size="sm" onClick={dismiss}>
            {t('update.bannerLater')}
          </Button>
        </>
      }
    >
      <p className="m-0 [overflow-wrap:anywhere]">
        {t('update.banner', { version })}
      </p>
    </Alert>
  );
}
