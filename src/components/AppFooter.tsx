import { Separator } from '@/components/ui/separator';
import { useAppInfo } from '@/hooks/useAppInfo';
import { useT } from '@/i18n/useT';

const NO_VALUE = '—';

export function AppFooter() {
  const t = useT();
  const info = useAppInfo();
  return (
    <footer className="flex items-center justify-center gap-3 border-t border-cr-border px-6 py-3 text-caption tabular-nums text-cr-text-secondary">
      <span>{t('footer.version', { version: info?.version ?? NO_VALUE })}</span>
      <Separator orientation="vertical" className="h-3" />
      <span>
        {t('footer.coreVersion', { version: info?.core_version ?? NO_VALUE })}
      </span>
    </footer>
  );
}
