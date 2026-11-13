import { useT } from '@/i18n/useT';
import type { GameDetailsView } from '@/ipc/types';

/**
 * The head of the game panel (addon views §8.8 1): the system and the
 * region the product code gives («3DS, Europa, España»), and the title id
 * in mono. The region line is left out when the name already carries a
 * region (the name wins, as in Organizar) or when the facts did not load;
 * a letter the verified table does not know is said as it is, never
 * guessed («Letra de región «T» sin identificar»).
 */
export function RegionLine({
  systemName,
  nameHasRegion,
  details,
}: {
  systemName: string;
  /** The entry's name already carries a region (`EntryView.regions`). */
  nameHasRegion: boolean;
  details: GameDetailsView | null;
}) {
  const t = useT();
  if (!details) return null;
  const verdict = nameHasRegion ? undefined : details.region;
  let line: string | null = null;
  let unknown: string | null = null;
  if (verdict?.certainty === 'unknown' || (verdict && !verdict.region)) {
    unknown = t('panel.regionUnknown', {
      letter: verdict.letter,
      code: verdict.code,
    });
  } else if (verdict?.region) {
    const vars = {
      system: systemName,
      region: t(`regions.${verdict.region}`),
    };
    line =
      verdict.certainty === 'likely'
        ? t('panel.regionLikely', vars)
        : verdict.country
          ? t('panel.regionCountry', {
              ...vars,
              country: t(`regions.${verdict.country}`),
            })
          : t('panel.region', vars);
  }
  if (!line && !unknown && !details.title_id) return null;
  return (
    <div className="panel-head">
      {line && <p className="panel-head__region">{line}</p>}
      {unknown && <p className="panel-head__unknown">{unknown}</p>}
      {details.title_id && (
        <p className="panel-head__tid">
          <span className="sr-only">{t('panel.titleId')}: </span>
          <span className="mono tabular-nums">{details.title_id}</span>
        </p>
      )}
    </div>
  );
}
