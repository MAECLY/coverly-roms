import { useT } from '@/i18n/useT';
import { useLibraryStore } from '@/stores/libraryStore';

export function LibraryScreen() {
  const t = useT();
  const view = useLibraryStore((s) => s.view);
  const close = useLibraryStore((s) => s.close);
  if (!view) return null;
  return (
    <div className="flex min-h-screen flex-col">
      <header className="toolbar">
        <h1 className="toolbar-name truncate" title={view.root}>
          {view.name}
        </h1>
        <button type="button" className="ghost-button" onClick={close}>
          {t('library.close')}
        </button>
      </header>
      <main className="flex-1 p-6">
        <p>{t('library.count', { n: view.entries.length })}</p>
      </main>
    </div>
  );
}
