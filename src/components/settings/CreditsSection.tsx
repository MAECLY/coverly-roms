import { ExternalLink } from 'lucide-react';
import { useContext, useMemo, useState } from 'react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import type { CreditLink } from '@/ipc/types';
import {
  CREDITS,
  creditCount,
  searchCredits,
  type CreditView,
} from '@/lib/credits';

import { Field, hintText } from './Field';
import { NoticesDialog } from './NoticesDialog';

/** The one «opens in your browser» line every credit link points at. */
const OPENS_ID = 'credits-opens-browser';

function open(id: string, which: CreditLink) {
  void getBackend().then((b) => b.openCredit(id, which));
}

function CreditRow({ c }: { c: CreditView }) {
  const t = useT();
  const nameId = `credit-${c.id}-name`;
  return (
    <article
      id={`credit-${c.id}`}
      aria-labelledby={nameId}
      tabIndex={-1}
      data-setting-row=""
      className="credits__entry"
    >
      <div className="credits__head">
        <h4 id={nameId} className="credits__name">
          {c.name}
        </h4>
        {c.license.url ? (
          <Button
            variant="link"
            size="sm"
            className="credits__license"
            aria-label={`${t('settings.credits.license', { name: c.name })}: ${c.license.name}`}
            aria-describedby={OPENS_ID}
            onClick={() => open(c.id, 'license')}
          >
            {c.license.name}
          </Button>
        ) : (
          <span className="credits__license">{c.license.name}</span>
        )}
      </div>
      <p className="credits__use">{c.use}</p>
      {c.credit && (
        <blockquote className="credits__quote">{c.credit}</blockquote>
      )}
      {c.url && (
        <Button
          variant="link"
          size="sm"
          className="self-start"
          aria-describedby={OPENS_ID}
          onClick={() => open(c.id, 'site')}
        >
          {t('settings.credits.site', { name: c.name })}
          <ExternalLink aria-hidden className="size-3.5" />
        </Button>
      )}
    </article>
  );
}

/** «Agradecimientos» (spec §9.1): the app's colophon, one reading column of
 * groups headed by the shelf rule, fed by `credits/credits.json`. */
export function CreditsSection() {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const [query, setQuery] = useState('');
  const groups = useMemo(() => searchCredits(query, locale), [query, locale]);
  const shown = creditCount(groups);
  return (
    <div className="credits">
      <h2 className="credits__title">{t('settings.section.credits')}</h2>
      <p className="credits__intro">{t('settings.credits.intro')}</p>
      <Field id="setting-credits-search" label={t('settings.credits.search')}>
        <Input
          type="search"
          name="credits-search"
          autoComplete="off"
          spellCheck={false}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </Field>
      <p aria-live="polite" data-testid="credits-count" className={hintText}>
        {shown === 0
          ? t('settings.credits.empty', { query: query.trim() })
          : tr('settings.credits.results', {
              count: shown,
              total: CREDITS.length,
            })}
      </p>
      <span id={OPENS_ID} hidden>
        {t('settings.credits.opensBrowser')}
      </span>
      {groups.map((g) => {
        const headId = `credits-group-${g.group}`;
        return (
          <section
            key={g.group}
            aria-labelledby={headId}
            className="credits__group"
          >
            <h3 id={headId} className="credits__shelf">
              <span>{t(`settings.credits.groups.${g.group}`)}</span>{' '}
              {/* The space keeps «Libraries 12» two words for assistive
                  tech; a flex container does not render it. */}
              <span className="font-mono tabular-nums credits__count">
                {g.entries.length}
              </span>
            </h3>
            {g.entries.map((c) => (
              <CreditRow key={c.id} c={c} />
            ))}
          </section>
        );
      })}
      <div className="credits__footer">
        <NoticesDialog>
          <Button
            id="setting-credits-notices"
            variant="secondary"
            className="credits__notices"
          >
            {t('settings.credits.notices')}
          </Button>
        </NoticesDialog>
        <p className="m-0">{t('settings.credits.noContent')}</p>
      </div>
    </div>
  );
}
