import { Languages } from 'lucide-react';

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  type SelectSize,
} from '@/components/ui/select';
import { useT } from '@/i18n/useT';
import type { LanguageChoice } from '@/ipc/types';
import { cn } from '@/lib/utils';
import { useSettingsStore } from '@/stores/settingsStore';

const CHOICES: readonly LanguageChoice[] = ['auto', 'en', 'es'];

/**
 * The language setting (General → Language), kept in the header. `size`
 * matches the neighbouring controls: compact on the start screen header,
 * the default 32px in the library toolbar.
 */
export function LanguageSwitch({
  size = 'sm',
  className,
  valueClassName,
}: {
  size?: SelectSize;
  className?: string;
  /** Wraps the shown value, e.g. to hide it on narrow windows. */
  valueClassName?: string;
}) {
  const t = useT();
  const choice = useSettingsStore(
    (s) => s.settings?.general.language ?? 'auto',
  );
  const ready = useSettingsStore((s) => s.status === 'ready');
  return (
    <Select
      name="language"
      value={choice}
      disabled={!ready}
      onValueChange={(value) =>
        void useSettingsStore
          .getState()
          .update({ general: { language: value as LanguageChoice } })
      }
    >
      <SelectTrigger
        size={size}
        aria-label={t('language.label')}
        className={cn('w-auto min-w-[9.5rem] gap-1.5', className)}
      >
        <Languages aria-hidden className="size-3.5 text-cr-text-secondary" />
        {valueClassName ? (
          <span className={valueClassName}>
            <SelectValue />
          </span>
        ) : (
          <SelectValue />
        )}
      </SelectTrigger>
      <SelectContent align="end">
        {CHOICES.map((c) => (
          <SelectItem key={c} value={c}>
            {t(`language.${c}`)}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
