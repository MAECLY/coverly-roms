import { useT } from '@/i18n/useT';
import { useNavStore } from '@/stores/navStore';
import { useOrganizeStore } from '@/stores/organizeStore';

/** The organise proposal for a loose pile (organise spec §4.2). */
export function OrganizeScreen() {
  const t = useT();
  const source = useNavStore((s) => s.organizeSource);
  const back = () => {
    useOrganizeStore.getState().reset();
    useNavStore.getState().closeOrganize();
  };
  return (
    <main className="settings-screen">
      <header className="settings-header">
        <button type="button" className="ghost-button" onClick={back}>
          {t('organize.back')}
        </button>
        <h1>{t('organize.title')}</h1>
      </header>
      {source && <p className="hint">{source}</p>}
    </main>
  );
}
