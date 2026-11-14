import { ExternalLink, FolderOpen } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { useT } from '@/i18n/useT';
import type { Playable } from '@/ipc/types';
import { revealLabelKey } from '@/lib/reveal';

/**
 * What the viewer says about a file it does not show (extras D17):
 * chiptunes, packages, executables, a media error, a refused grant, a
 * file over 200 MB or an image over 100 megapixels. The per-OS reveal is
 * always there; «Abrir con otra app» only for a type the backend allows
 * (never an executable, script, shortcut or package: `reveal-only`). No
 * emulator or player is named.
 */
export function UnplayableView({
  name,
  playable,
  onReveal,
  onOpenExternal,
}: {
  name: string;
  playable: Playable;
  onReveal(): void;
  onOpenExternal(): void;
}) {
  const t = useT();
  return (
    <div className="media-viewer__stage">
      <div className="media-viewer__unplayable">
        <p className="media-viewer__unplayable-name mono" title={name}>
          {name}
        </p>
        <p className="media-viewer__unplayable-text">
          {t('viewer.unplayable')}
        </p>
        <div className="media-viewer__unplayable-actions">
          {playable !== 'reveal-only' && (
            <Button size="sm" variant="secondary" onClick={onOpenExternal}>
              <ExternalLink aria-hidden />
              {t('viewer.openExternal')}
            </Button>
          )}
          <Button size="sm" variant="outline" onClick={onReveal}>
            <FolderOpen aria-hidden />
            {t(revealLabelKey())}
          </Button>
        </div>
      </div>
    </div>
  );
}
