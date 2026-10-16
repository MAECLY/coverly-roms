import { useT } from '@/i18n/useT';
import { useNavStore } from '@/stores/navStore';
import { useUpdateStore } from '@/stores/updateStore';

/** One line when the startup check found an update; never for errors. */
export function UpdateBanner() {
  const t = useT();
  const show = useUpdateStore(
    (s) =>
      s.status === 'available' && s.trigger === 'startup' && !s.bannerDismissed,
  );
  const version = useUpdateStore((s) => s.offer?.version ?? '');
  const dismiss = useUpdateStore((s) => s.dismissBanner);
  if (!show) return null;
  return (
    <div className="update-banner" role="status">
      <span>{t('update.banner', { version })}</span>
      <button
        type="button"
        className="ghost-button"
        onClick={() => {
          dismiss();
          useNavStore.getState().openSettings('about');
        }}
      >
        {t('update.bannerDetails')}
      </button>
      <button type="button" className="ghost-button" onClick={dismiss}>
        {t('update.bannerLater')}
      </button>
    </div>
  );
}
