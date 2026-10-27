import { Settings } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useNavStore, type SettingsSection } from '@/stores/navStore';

/**
 * «Ajustes de …» in a screen's header: opens Settings on `section`. Back
 * from Settings returns to the screen with its state and focuses this
 * button again (`data-settings-opener`, see `App`).
 */
export function SettingsLink({
  section,
  label,
  disabled,
  onOpen,
}: {
  section: SettingsSection;
  label: string;
  disabled?: boolean;
  /** Runs before Settings opens (the screen stamps its settings). */
  onOpen?: () => void;
}) {
  return (
    <Button
      size="sm"
      variant="ghost"
      className="ml-auto"
      data-settings-opener
      disabled={disabled ?? false}
      onClick={() => {
        onOpen?.();
        useNavStore.getState().openSettings(section);
      }}
    >
      <Settings aria-hidden />
      {label}
    </Button>
  );
}
