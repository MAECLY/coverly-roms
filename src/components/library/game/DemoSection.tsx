import { useContext } from 'react';

import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type { EntryView } from '@/ipc/types';
import { formatSize } from '@/lib/format';
import type { Game } from '@/lib/games';

import { ArtThumb } from '../ArtThumb';

/** «Paper Skies (Demo)» → «Paper Skies»: the full game's name, when the
 * library does not have it and Rust sent none. */
function withoutTags(name: string): string {
  return name.replace(/\s*[([][^)\]]*[)\]]/g, '').trim() || name;
}

/**
 * «Demo relacionada» on a full game, and «Demo» on a demo (addon views
 * §8.8 5, D8). A demo is a game of its own: on the full game its card
 * («Juego aparte, 184 MB») selects it; on the demo, a card opens the full
 * game, or a sentence says the library does not have it. A demo without a
 * cover shows the full game's, dimmed and labelled (display only, never
 * written to disk).
 */
export function DemoSection({
  generation,
  game,
  entries,
  systemId,
  onSelect,
}: {
  generation: number;
  game: Game;
  /** The library's entries, by id (the demos and the full game). */
  entries: readonly EntryView[];
  systemId: string | null | undefined;
  onSelect(entryId: number): void;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const entry = game.entry;

  if (game.isDemo) {
    const full = game.demoOf !== null ? entries[game.demoOf] : undefined;
    const fullName =
      full?.name ?? entry.demo?.of_name ?? withoutTags(entry.name);
    const borrowCover =
      full !== undefined &&
      entry.art.cover !== 'present' &&
      full.art.cover === 'present';
    return (
      <section
        id="panel-demo"
        data-panel-section="demo"
        className="panel-sec"
        aria-labelledby="panel-demo-h"
      >
        <h3 id="panel-demo-h" className="panel-sec__h">
          {t('panel.demo')}
        </h3>
        {borrowCover && (
          <figure className="panel-borrowed">
            <ArtThumb
              generation={generation}
              entry={full}
              media="cover"
              systemId={systemId}
              className="panel-borrowed__art"
            />
            <figcaption className="panel-note">
              {t('panel.fromFull')}
            </figcaption>
          </figure>
        )}
        {full ? (
          <button
            type="button"
            className="panel-card"
            onClick={() => onSelect(full.id)}
          >
            <span aria-hidden="true" className="panel-card__art">
              <ArtThumb
                generation={generation}
                entry={full}
                media="cover"
                placeholder=""
                systemId={systemId}
              />
            </span>
            <span className="panel-card__text">
              <span className="panel-card__title">{t('panel.openFull')}</span>
              <span className="panel-card__sub">
                {t('panel.fullInLibrary', { name: full.name })}
              </span>
            </span>
          </button>
        ) : (
          <p className="panel-text">
            {t('panel.fullMissing', { name: fullName })}
          </p>
        )}
        <p className="panel-note">{t('panel.demoNote')}</p>
      </section>
    );
  }

  const demos = game.demos
    .map((id) => entries[id])
    .filter((e): e is EntryView => e !== undefined);
  if (demos.length === 0) return null;
  return (
    <section
      id="panel-demo"
      data-panel-section="demo"
      className="panel-sec"
      aria-labelledby="panel-demo-h"
    >
      <h3 id="panel-demo-h" className="panel-sec__h">
        {t('panel.relatedDemo')}
      </h3>
      {demos.map((d) => (
        <button
          key={d.id}
          type="button"
          className="panel-card"
          onClick={() => onSelect(d.id)}
        >
          <span aria-hidden="true" className="panel-card__art">
            <ArtThumb
              generation={generation}
              entry={d.art.cover === 'present' ? d : entry}
              media="cover"
              placeholder=""
              systemId={systemId}
              className={d.art.cover === 'present' ? '' : 'panel-borrowed__art'}
            />
          </span>
          <span className="panel-card__text">
            <span className="panel-card__title">{d.name}</span>
            <span className="panel-card__sub">
              {t('panel.demoCard', { size: formatSize(d.size, locale) })}
            </span>
          </span>
        </button>
      ))}
    </section>
  );
}
