import type { ExtrasTextView } from '@/ipc/types';
import { useT } from '@/i18n/useT';

/**
 * A text extra (txt, md, nfo; extras D17): React text nodes in a `<pre>`,
 * never HTML, so an `.nfo` holding `<script>` shows it literally. NFO art
 * is drawn in CP437 box characters, so it keeps the mono face; plain notes
 * read in the body face. Says when only the first 1 MiB is shown.
 */
export function TextView({
  view,
  mono,
}: {
  view: ExtrasTextView;
  mono: boolean;
}) {
  const t = useT();
  return (
    <div className="media-viewer__stage media-viewer__stage--text">
      {view.truncated && (
        <p className="media-viewer__note">{t('viewer.truncated')}</p>
      )}
      {/* eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- a long text scrolls; the keyboard reaches it */}
      <pre className={`media-viewer__text${mono ? ' mono' : ''}`} tabIndex={0}>
        {view.text}
      </pre>
    </div>
  );
}
