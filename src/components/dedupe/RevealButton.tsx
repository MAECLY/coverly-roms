/**
 * "Show in Finder / Explorer / its folder" for one copy or the duplicates
 * folder. An icon button: the tooltip says the OS words, the accessible
 * name is the whole sentence with the file ("Mostrar Tetris.gb en
 * Finder"), never a glued fragment. The path goes to `reveal_path`, which
 * only shows places inside the library, a kept plan or its quarantine.
 */
import { FolderSearch } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { SimpleTooltip } from '@/components/ui/tooltip';
import { useT } from '@/i18n/useT';
import {
  revealCopyKey,
  revealFolderKey,
  revealLabelKey,
  revealOs,
  revealShortcut,
} from '@/lib/reveal';

export interface RevealButtonProps {
  /** The copy's current place; `null` hides the button (Trash apply). */
  path: string | null;
  /** File or folder name for the sentence. */
  name: string;
  onReveal: (path: string) => void;
  /** Advertise the reveal chord (`aria-keyshortcuts`): on the focused
   * group's rows. */
  shortcut?: boolean;
  kind?: 'copy' | 'folder';
  className?: string;
}

export function RevealButton({
  path,
  name,
  onReveal,
  shortcut = false,
  kind = 'copy',
  className,
}: RevealButtonProps) {
  const t = useT();
  if (path === null) return null;
  const os = revealOs();
  const label =
    kind === 'folder'
      ? t(revealFolderKey(os), { folder: name })
      : t(revealCopyKey(os), { name });
  return (
    <SimpleTooltip content={t(revealLabelKey(os))}>
      <Button
        variant="ghost"
        size="icon-sm"
        className={className ? `dup-reveal ${className}` : 'dup-reveal'}
        aria-label={label}
        aria-keyshortcuts={shortcut ? revealShortcut(os).aria : undefined}
        data-reveal=""
        onClick={() => onReveal(path)}
      >
        <FolderSearch aria-hidden />
      </Button>
    </SimpleTooltip>
  );
}
