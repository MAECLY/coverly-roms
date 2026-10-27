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
  describedBy,
  onOpen,
}: {
  section: SettingsSection;
  label: string;
  disabled?: boolean;
  /** An alert the screen shows about its settings: focus comes back
   * here from Settings, so the link reads it (it mounts with the screen,
   * too early for a live region). */
  describedBy?: string | undefined;
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
      aria-describedby={describedBy}
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
