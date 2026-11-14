import {
  Clapperboard,
  FileText,
  Image as ImageIcon,
  Music,
  Package,
  Tag,
} from 'lucide-react';
import { useContext, useState } from 'react';

import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { NativeSelect } from '@/components/ui/select';
import { LocaleContext } from '@/i18n/core';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import { errorKey, type BackendError } from '@/ipc/errors';
import type {
  AddChoice,
  AddFileView,
  AddPlanView,
  CategoryRef,
  ExtraType,
} from '@/ipc/types';
import {
  ADD_BYTES,
  ADD_FILES,
  categoryKey,
  categoryLabel,
  extOf,
  fitsCategory,
  nameKey,
} from '@/lib/extras';
import { formatSize } from '@/lib/format';

const GLYPH: Record<ExtraType, typeof Music> = {
  audio: Music,
  video: Clapperboard,
  image: ImageIcon,
  document: FileText,
  other: Package,
};

/** What the add dialog confirms (D13): a plan from the picker or a drop. */
export interface AddRequest {
  plan: AddPlanView;
  /** A category's «+»: every file starts there. */
  category?: CategoryRef | undefined;
  /** An OS drop on this category: the files that fit start there. */
  dropped?: CategoryRef | undefined;
  /** An OS drop on no category: the dialog asks which one. */
  ask?: boolean | undefined;
}

/** Tags chosen from a rip's own tags, by plan file index. */
export type AddTags = Record<number, string[]>;

function startCategory(file: AddFileView, req: AddRequest): CategoryRef {
  if (req.category) return req.category;
  if (req.dropped && fitsCategory(file.name, req.dropped)) return req.dropped;
  return { kind: 'fixed', name: file.category };
}

/** The rip's own tags a file may take, without repeats. */
function ripOffers(file: AddFileView): string[] {
  const out: string[] = [];
  for (const v of [file.rip?.title, file.rip?.artist]) {
    const tag = typeof v === 'string' ? v.trim() : '';
    if (tag && !out.some((o) => nameKey(o) === nameKey(tag))) out.push(tag);
  }
  return out;
}

/** The bytes a `no-space` error says are short (`needed/free`). */
function shortBy(detail: string | undefined): number | null {
  const m = /^(\d+)\/(\d+)$/.exec(detail ?? '');
  if (!m) return null;
  return Math.max(0, Number(m[1]) - Number(m[2]));
}

/**
 * The add step (extras spec D13, §8.3): what will be copied, the category
 * of each file (suggested by type, changeable), a rip's own tags offered
 * as buttons (never applied unless pressed), the total in mono and the
 * note that the originals stay. Files are named by plan index, never by
 * path (D22).
 */
export function ExtrasAddDialog({
  request,
  targets,
  onCommit,
  onClose,
}: {
  request: AddRequest | null;
  targets: readonly CategoryRef[];
  /** Copies; answers the error to show, or null when it started. */
  onCommit(choices: AddChoice[], tags: AddTags): Promise<BackendError | null>;
  onClose(): void;
}) {
  return (
    <Dialog
      open={request !== null}
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
    >
      {request && (
        <AddContent
          // A new plan starts from its own suggestions.
          key={request.plan.plan}
          request={request}
          targets={targets}
          onCommit={onCommit}
          onClose={onClose}
        />
      )}
    </Dialog>
  );
}

function AddContent({
  request,
  targets,
  onCommit,
  onClose,
}: {
  request: AddRequest;
  targets: readonly CategoryRef[];
  onCommit(choices: AddChoice[], tags: AddTags): Promise<BackendError | null>;
  onClose(): void;
}) {
  const t = useT();
  const tr = useRichT();
  const locale = useContext(LocaleContext);
  const { plan } = request;
  const [chosen, setChosen] = useState<Record<number, CategoryRef>>(() =>
    Object.fromEntries(
      plan.files.map((f) => [f.index, startCategory(f, request)]),
    ),
  );
  const [left, setLeft] = useState<ReadonlySet<number>>(new Set());
  const [tags, setTags] = useState<AddTags>({});
  const [error, setError] = useState<BackendError | null>(null);
  const [busy, setBusy] = useState(false);
  const num = new Intl.NumberFormat(locale);
  const size = (n: number) => formatSize(n, locale);

  const kept = plan.files.filter((f) => !left.has(f.index));
  const n = kept.length;
  const total = kept.reduce((s, f) => s + f.size, 0);
  const tooMany =
    plan.too_many ||
    (error?.code === 'extras-refused' && error.detail === 'too-many');

  async function commit() {
    setBusy(true);
    setError(null);
    const choices = kept.map((f) => ({
      index: f.index,
      category: chosen[f.index]!,
    }));
    const picked: AddTags = {};
    for (const f of kept)
      if (tags[f.index]?.length) picked[f.index] = tags[f.index]!;
    const err = await onCommit(choices, picked);
    setBusy(false);
    if (err) setError(err);
  }

  function toggleTag(index: number, tag: string) {
    setTags((all) => {
      const had = all[index] ?? [];
      const on = had.some((x) => nameKey(x) === nameKey(tag));
      return {
        ...all,
        [index]: on
          ? had.filter((x) => nameKey(x) !== nameKey(tag))
          : [...had, tag],
      };
    });
  }

  function errorText(err: BackendError): string {
    if (err.code === 'no-space') {
      const short = shortBy(err.detail);
      if (short !== null) return t('extras.noSpace', { size: size(short) });
    }
    return t(errorKey(err.code), { detail: err.detail ?? '' });
  }

  return (
    <DialogContent size="lg" className="extras-add">
      <DialogHeader>
        <DialogTitle>
          {request.ask ? t('extras.whichCategory') : t('extras.add')}
        </DialogTitle>
        <DialogDescription>{t('extras.addNote')}</DialogDescription>
      </DialogHeader>
      <DialogBody>
        <ul className="extras-add__list">
          {plan.files.map((f) => {
            const Glyph = GLYPH[f.kind];
            const out = left.has(f.index);
            const offers = ripOffers(f);
            const game =
              typeof f.rip?.game === 'string' ? f.rip.game.trim() : '';
            return (
              <li
                key={f.index}
                className="extras-add__file"
                data-left={out || undefined}
              >
                <Checkbox
                  className="extras-add__keep"
                  checked={!out}
                  aria-label={t('extras.addInclude', { name: f.name })}
                  onCheckedChange={(on) =>
                    setLeft((s) => {
                      const next = new Set(s);
                      if (on === true) next.delete(f.index);
                      else next.add(f.index);
                      return next;
                    })
                  }
                />
                <Glyph aria-hidden className="extras-add__glyph" />
                <span className="extras-add__name" title={f.name}>
                  {f.name}
                </span>
                <span className="extras-add__size mono">{size(f.size)}</span>
                <NativeSelect
                  size="sm"
                  className="extras-add__cat"
                  aria-label={t('extras.categoryFor', { name: f.name })}
                  disabled={out}
                  value={categoryKey(chosen[f.index]!)}
                  onChange={(e) => {
                    const to = targets.find(
                      (c) => categoryKey(c) === e.target.value,
                    );
                    if (to) setChosen((c) => ({ ...c, [f.index]: to }));
                  }}
                >
                  {targets.map((c) => (
                    <option key={categoryKey(c)} value={categoryKey(c)}>
                      {categoryLabel(c, t)}
                    </option>
                  ))}
                </NativeSelect>
                {(game || offers.length > 0) && (
                  <div className="extras-add__rip">
                    {game && (
                      <span className="extras-add__ripgame">
                        {t('extras.ripTag', {
                          format: extOf(f.name).toUpperCase(),
                          game,
                        })}
                      </span>
                    )}
                    {offers.map((tag) => {
                      const on = (tags[f.index] ?? []).some(
                        (x) => nameKey(x) === nameKey(tag),
                      );
                      return (
                        <button
                          key={tag}
                          type="button"
                          className="extras-add__tag"
                          aria-pressed={on}
                          aria-label={t('extras.ripTagAdd', { tag })}
                          disabled={out}
                          onClick={() => toggleTag(f.index, tag)}
                        >
                          <Tag aria-hidden />
                          {tag}
                        </button>
                      );
                    })}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
        {(plan.keys > 0 || plan.hidden > 0) && (
          <p className="panel-note extras-add__skipped">
            {plan.keys === 1 && t('extras.keysSkippedOne')}
            {plan.keys > 1 &&
              tr('extras.keysSkipped', { n: num.format(plan.keys) })}
            {plan.keys > 0 && plan.hidden > 0 && ' '}
            {plan.hidden === 1 && t('extras.hiddenSkippedOne')}
            {plan.hidden > 1 &&
              tr('extras.hiddenSkipped', { n: num.format(plan.hidden) })}
          </p>
        )}
        {tooMany && (
          <Alert variant="warning" className="extras-alert">
            {tr('extras.tooMany', {
              files: num.format(ADD_FILES),
              size: size(ADD_BYTES),
            })}
          </Alert>
        )}
        {error && !tooMany && (
          <Alert variant="danger" className="extras-alert">
            {errorText(error)}
          </Alert>
        )}
      </DialogBody>
      <DialogFooter className="extras-add__foot">
        <span className="extras-add__total">
          {tr('extras.addTotal', { size: size(total) })}
        </span>
        <Button variant="ghost" onClick={onClose}>
          {t('extras.cancel')}
        </Button>
        <Button
          variant="primary"
          disabled={n === 0 || tooMany || busy}
          onClick={() => void commit()}
        >
          {n === 1
            ? t('extras.addConfirmOne')
            : t('extras.addConfirm', { n: num.format(n) })}
        </Button>
      </DialogFooter>
    </DialogContent>
  );
}
