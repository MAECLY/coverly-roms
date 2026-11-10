import { Archive, ChevronRight, Folder } from 'lucide-react';
import { useContext, useEffect, useId, useMemo, useRef, useState } from 'react';

import { SystemIcon } from '@/components/system/SystemIcon';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { ScrollArea } from '@/components/ui/scroll-area';
import { NativeSelect } from '@/components/ui/select';
import { LocaleContext } from '@/i18n/core';
import { useT } from '@/i18n/useT';
import type {
  EmulatorChoice,
  EmulatorId,
  OrganizeItem,
  ProfileFile,
  SystemInfo,
  TransferMode,
  TranslatedGameView,
} from '@/ipc/types';
import { formatDecimalSize } from '@/lib/format';
import {
  ROWS_PER_GROUP,
  actionOf,
  childTarget,
  confidenceKey,
  defaultEmulator,
  emulatorName,
  emulatorsFor,
  extractDetail,
  isOwner,
  licenceLine,
  playlistKey,
  rowNotes,
  systemDir,
  translatedFor,
  whyKey,
  type SystemGroup,
} from '@/lib/organize';
import { systemOptionGroups } from '@/lib/systems';

import { ExtractDetail } from './ExtractDetail';
import { GameFolderPreview } from './GameFolderPreview';
import { AddonLine, ItemName, TitleIdLine } from './ItemName';
import { NewBadge, NewCount, newRow, type IsNew } from './NewMarks';
import type { OrganizeChoice } from './ReviewBuckets';
import {
  chevron,
  fileList,
  fileRow,
  groupTrigger,
  hintText,
  listViewport,
  panel,
} from './styles';

const LEAVE = '__leave__';

interface Props {
  groups: SystemGroup[];
  /** Display name of the target layout (for "no folder on device"). */
  layoutName: string;
  systemName(id: string): string;
  /** Every known system: a system the user chose can be changed again. */
  systems?: SystemInfo[];
  disabled?: boolean;
  onDecide?(itemIds: number[], choice: OrganizeChoice): void;
  /** Files the last "scan again" found. */
  isNew?: IsNew;
  /** The plan's transfer mode: what applying does with as-is items. */
  mode?: TransferMode;
  /** The target layout, for where a disc's tracks go and the emulator
   * each system gets by default. */
  profile?: Pick<ProfileFile, 'disc_layout' | 'emulators'> | null;
  /** Moving items whose licence the core left in place
   * (`licencesStaying`). */
  licenceStays?: ReadonlySet<number>;
  /** The emulator chosen per system for this run (mods spec D16). */
  emulators?: Readonly<Record<string, EmulatorChoice>>;
  /** Picks a system's emulator; `null` takes the layout's default. No
   * handler, no select. */
  onEmulator?(system: string, choice: EmulatorChoice | null): void;
  /** The plan's games laid out for their system's emulator. */
  translated?: readonly TranslatedGameView[];
}

interface RowProps {
  layoutName: string;
  mode?: TransferMode;
  profile?: Pick<ProfileFile, 'disc_layout' | 'emulators'> | null;
  systemName(id: string): string;
  systems?: SystemInfo[];
  disabled?: boolean;
  onDecide?(itemIds: number[], choice: OrganizeChoice): void;
  isNew?: IsNew;
  licenceStays?: ReadonlySet<number>;
  translated?: readonly TranslatedGameView[];
}

const NONE: ReadonlySet<number> = new Set();

/** The items that move, one collapsible per system folder (spec §4.2.3). */
export function SystemGroups({ groups, emulators, onEmulator, ...row }: Props) {
  const total = groups.reduce((n, g) => n + g.items.length, 0);
  // Large piles start closed so thousands of rows never reach the DOM at once.
  const startOpen = total <= ROWS_PER_GROUP;
  return (
    <div className="flex flex-col gap-2">
      {/* The folder is a stable key: a re-plan after an emulator change
          keeps the group, and the select its focus (§9.1). */}
      {groups.map((g) => (
        <Group
          key={g.folder}
          group={g}
          startOpen={startOpen}
          chosen={emulators?.[g.systemId] ?? null}
          {...(onEmulator && { onEmulator })}
          {...row}
        />
      ))}
    </div>
  );
}

function Group({
  group,
  startOpen,
  chosen,
  onEmulator,
  ...row
}: RowProps & {
  group: SystemGroup;
  startOpen: boolean;
  chosen: EmulatorChoice | null;
  onEmulator?(system: string, choice: EmulatorChoice | null): void;
}) {
  const t = useT();
  const id = useId();
  const nameId = `${id}-system`;
  const [open, setOpen] = useState(startOpen);
  const [visible, setVisible] = useState(ROWS_PER_GROUP);
  const rows = group.items.slice(0, visible);
  const rest = group.items.length - rows.length;
  const dir = systemDir(group.items[0]!);
  const isNew = row.isNew;
  const fresh = isNew ? group.items.filter(isNew).length : 0;
  const noEmulator =
    row.systems?.find((s) => s.id === group.systemId)?.no_emulator === true;
  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      role="group"
      aria-labelledby={id}
      className={panel}
    >
      <div className="flex flex-wrap items-center gap-x-3">
        <div className="min-w-0 flex-[1_1_20rem]">
          <CollapsibleTrigger asChild>
            <Button id={id} variant="ghost" className={groupTrigger}>
              <ChevronRight aria-hidden className={chevron} />
              <Folder aria-hidden className="text-cr-text-secondary" />
              <span className="font-mono text-ui text-cr-text [overflow-wrap:anywhere]">
                {`${dir}/ (${group.items.length})`}
              </span>{' '}
              {/* The spaces keep the parts apart in the accessible name; a
              flex row drops them from the layout. */}
              <SystemIcon systemId={group.systemId} size={16} />
              {/* Name and «Sin emulador» are one text run: a narrow window
              wraps them as a phrase, not as separate flex columns. */}
              <span className="text-caption font-normal text-cr-text-secondary">
                <span id={nameId}>{row.systemName(group.systemId)}</span>
                {noEmulator && (
                  // A fact about the world, not a fault: no box, no warning
                  // colour; read as part of the group's name (spec §9.1).
                  <>
                    {' '}
                    <span aria-hidden>·</span>{' '}
                    <span className="whitespace-nowrap">
                      {t('systems.noEmulator')}
                    </span>
                  </>
                )}
              </span>
              <NewCount count={fresh} />
            </Button>
          </CollapsibleTrigger>
        </div>
        {onEmulator && (
          <EmulatorField
            system={group.systemId}
            describedBy={nameId}
            chosen={chosen}
            fallback={defaultEmulator(row.profile, group.systemId)}
            disabled={row.disabled ?? false}
            onEmulator={onEmulator}
          />
        )}
      </div>
      {/* Rows mount only while open (Radix unmounts closed content). */}
      <CollapsibleContent className="border-t border-cr-border">
        <ScrollArea aria-label={`${dir}/`} viewportClassName={listViewport}>
          <ul className={fileList}>
            {rows.map((item) => (
              <Row key={item.id} item={item} group={group.items} {...row} />
            ))}
          </ul>
          {rest > 0 && (
            <div className="border-t border-cr-border px-3 py-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setVisible((v) => v + ROWS_PER_GROUP)}
              >
                {t('organize.showMore', { count: rest })}
              </Button>
            </div>
          )}
        </ScrollArea>
      </CollapsibleContent>
    </Collapsible>
  );
}

function Row({
  item,
  layoutName,
  systemName,
  systems,
  disabled = false,
  onDecide,
  isNew,
  mode = 'move',
  profile,
  licenceStays = NONE,
  translated,
  group,
}: RowProps & { item: OrganizeItem; group: readonly OrganizeItem[] }) {
  const t = useT();
  // A manual choice is the user's decision, not a detection level.
  const confidence =
    item.method === 'user' ? null : confidenceKey(item.confidence);
  const why = item.why[0];
  const reason = why ? whyKey(why, systemName, item.disc) : null;
  const name = item.sources[0] ?? '';
  // A system the user chose (now or remembered) stays changeable: a slip in
  // the review list must not need starting over.
  const changeable =
    item.method === 'user' && !!onDecide && !!systems && !!item.system_id;
  const fresh = isNew?.(item) ?? false;
  const locale = useContext(LocaleContext);
  const pickerGroups = useMemo(
    () => (changeable ? systemOptionGroups(systems ?? [], t, locale) : []),
    [changeable, systems, t, locale],
  );
  const serials = item.disc?.serials.join(', ') ?? '';
  const all = rowNotes(t, item, layoutName, systemName);
  // An addon says whose it is right under its name, before what it is
  // (mods spec §9.1); its other notes stay where they were.
  const owner =
    item.game?.addon && item.notes.some(isOwner)
      ? (all.inline[0] ?? null)
      : null;
  const notes = owner ? { ...all, inline: all.inline.slice(1) } : all;
  // A loose multi-disc set's new playlist (D24), under its notes.
  const playlist = playlistKey(item);
  const licence = licenceLine(item, licenceStays);
  // A linked update, DLC, mod or cheats shows only its fixed folder and
  // name (§9.1).
  const short = childTarget(item);
  // Its emulator's layout, also for a base without a game folder.
  const view = translatedFor(translated, item.id);
  return (
    <li className={fresh ? `${fileRow} ${newRow}` : fileRow}>
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className="grid min-w-0 flex-[1_1_16rem] gap-0.5">
          <ItemName
            item={item}
            target={short}
            {...(short !== item.target && { fullTarget: item.target })}
          />
          <VersionLine item={item} />
          <AddonLine item={item} owner={owner} />
          {serials && (
            <span className="font-mono text-caption tabular-nums text-cr-text-secondary">
              {serials}
            </span>
          )}
        </span>
        {fresh && <NewBadge />}
        <ActionBadge item={item} mode={mode} />
        {confidence && (
          <Badge
            variant={
              confidence === 'organize.confidence.high' ? 'success' : 'info'
            }
            title={t('organize.confidence.label')}
          >
            {t(confidence)}
          </Badge>
        )}
        {changeable && <SystemIcon systemId={item.system_id} size={16} />}
        {changeable && (
          <NativeSelect
            size="sm"
            className="w-auto max-w-[16rem]"
            id={`chosen-system-${item.id}`}
            name="system"
            aria-label={t('organize.review.systemFor', { name })}
            value={item.system_id ?? ''}
            disabled={disabled}
            onChange={(e) => {
              const v = e.target.value;
              if (!v || v === item.system_id) return;
              onDecide([item.id], v === LEAVE ? 'leave' : { system: v });
            }}
          >
            {/* Every system, so grouped by family like LeftInPlace (D18). */}
            {pickerGroups.map((g) => (
              <optgroup key={g.family} label={g.label}>
                {g.systems.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </optgroup>
            ))}
            <option value={LEAVE}>{t('organize.review.leave')}</option>
          </NativeSelect>
        )}
      </div>
      {reason && <p className={hintText}>{t(reason.key, reason.vars)}</p>}
      <TitleIdLine item={item} />
      {notes.inline.map((line) => (
        <p key={line} className={hintText}>
          {line}
        </p>
      ))}
      {playlist && <p className={hintText}>{t(playlist.key, playlist.vars)}</p>}
      {licence && <p className={hintText}>{t(licence.key, licence.vars)}</p>}
      {(item.game?.role === 'base' || view) && (
        <GameFolderPreview base={item} items={group} translated={view} />
      )}
      {/* An extract row folds the archive's own facts into its detail. */}
      <ExtractDetail
        item={item}
        name={name}
        profile={profile}
        notes={notes.detail}
      />
    </li>
  );
}

/** The default option's value: it sends nothing for the system. */
const DEFAULT = '';

/**
 * The emulator a system is laid out for (mods spec §9.1): a small
 * labelled select in the group header, only for a system with translators.
 * «Predeterminado: {emulator}» sends nothing; each emulator and «Ninguno»
 * are explicit. Without a layout default, «Ninguno» is the default. Focus
 * comes back to it after the re-plan it started (disabled meanwhile).
 */
function EmulatorField({
  system,
  describedBy,
  chosen,
  fallback,
  disabled,
  onEmulator,
}: {
  system: string;
  describedBy: string;
  chosen: EmulatorChoice | null;
  fallback: EmulatorId | null;
  disabled: boolean;
  onEmulator(system: string, choice: EmulatorChoice | null): void;
}) {
  const t = useT();
  const id = `${useId()}emulator-${system}`;
  const ref = useRef<HTMLSelectElement>(null);
  const refocus = useRef(false);
  const options = emulatorsFor(system);
  useEffect(() => {
    if (disabled || !refocus.current) return;
    refocus.current = false;
    const el = ref.current;
    const active = document.activeElement;
    // Only when disabling dropped focus to the page, never from elsewhere.
    if (el && (active === null || active === document.body || active === el))
      el.focus();
  }, [disabled]);
  if (options.length === 0) return null;
  const none = t('organize.emulator.none');
  const value =
    chosen === null || (chosen === 'none' && !fallback) ? DEFAULT : chosen;
  return (
    <div className="flex items-center gap-2 px-3 py-1.5">
      <label htmlFor={id} className="text-caption text-cr-text-secondary">
        {t('organize.emulator.label')}
      </label>
      <NativeSelect
        ref={ref}
        size="sm"
        className="w-auto max-w-[16rem]"
        id={id}
        name="emulator"
        aria-describedby={describedBy}
        value={value}
        disabled={disabled}
        onChange={(e) => {
          const v = e.target.value;
          if (v === value) return;
          refocus.current = true;
          onEmulator(system, v === DEFAULT ? null : (v as EmulatorChoice));
        }}
      >
        {fallback && (
          <option value={DEFAULT}>
            {t('organize.emulator.default', {
              emulator: emulatorName(fallback),
            })}
          </option>
        )}
        {options.map((e) => (
          <option key={e} value={e}>
            {emulatorName(e)}
          </option>
        ))}
        <option value={fallback ? 'none' : DEFAULT}>{none}</option>
      </NativeSelect>
    </div>
  );
}

/**
 * An update's or DLC's version (game folders D11, D24): `v1.1.0` in mono,
 * read as «versión 1.1.0», then «la más nueva aquí» as text, never colour.
 */
function VersionLine({ item }: { item: OrganizeItem }) {
  const t = useT();
  const game = item.game;
  if (!game?.version || (game.role !== 'update' && game.role !== 'dlc'))
    return null;
  return (
    <span className="text-caption text-cr-text-secondary">
      <span aria-hidden className="font-mono tabular-nums text-cr-text">
        {`v${game.version}`}
      </span>
      <span className="sr-only">
        {t('organize.version', { version: game.version })}
      </span>
      {game.newest_here && (
        <>
          {' '}
          <span aria-hidden>·</span> <span>{t('organize.newestHere')}</span>
        </>
      )}
    </span>
  );
}

/**
 * What applying does with a row, in words: *Move* / *Copy*, or *Extract* as
 * a petrol outline with the archive icon followed by its file count and
 * bytes in mono. The word is always there; colour only repeats it.
 */
function ActionBadge({
  item,
  mode,
}: {
  item: OrganizeItem;
  mode: TransferMode;
}) {
  const t = useT();
  const locale = useContext(LocaleContext);
  const action = actionOf(item, mode);
  const detail = extractDetail(item);
  if (action !== 'extract')
    return <Badge variant="neutral">{t(`organize.action.${action}`)}</Badge>;
  return (
    <span className="inline-flex items-center gap-2">
      <Badge variant="outline" className="border-primary text-primary">
        <Archive aria-hidden />
        {t('organize.action.extract')}
      </Badge>
      {detail && (
        <span className="whitespace-nowrap font-mono text-caption tabular-nums text-cr-text-secondary">
          {t(
            detail.count === 1
              ? 'organize.extract.detailOne'
              : 'organize.extract.detail',
            {
              count: detail.count,
              bytes: formatDecimalSize(detail.bytes, locale),
            },
          )}
        </span>
      )}
    </span>
  );
}
