import { useState, type ReactNode } from 'react';

import { Textarea } from '@/components/ui/textarea';
import { useSettingsStore } from '@/stores/settingsStore';

import { Field, settingId, useFieldError } from './Field';

/** A list edited as text, committed on blur. */
export function ListSetting({
  path,
  label,
  hint,
  more,
  value,
  split,
  join,
  toPatch,
}: {
  path: string;
  label: string;
  hint: string;
  /** Long help behind «Más información». */
  more?: ReactNode;
  value: string[];
  split(text: string): string[];
  join(list: string[]): string;
  toPatch(
    list: string[],
  ): Parameters<ReturnType<typeof useSettingsStore.getState>['update']>[0];
}) {
  const id = settingId(path);
  const update = useSettingsStore((s) => s.update);
  const error = useFieldError(path);
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <Field id={id} label={label} hint={hint} more={more} error={error}>
      <Textarea
        name={id}
        mono
        rows={4}
        value={draft ?? join(value)}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={async () => {
          if (draft === null) return;
          const errors = await update(toPatch(split(draft)));
          if (errors.length === 0) setDraft(null);
        }}
      />
    </Field>
  );
}
