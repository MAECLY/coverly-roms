import { X } from 'lucide-react';
import {
  useId,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactElement,
} from 'react';

import { Input } from '@/components/ui/input';
import {
  Popover,
  PopoverAnchor,
  PopoverContent,
} from '@/components/ui/popover';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import { MAX_TAGS, nameKey, normalizeTags } from '@/lib/extras';

/** Suggestions shown at most (the game's other tags first). */
const SUGGEST = 8;

export interface ExtraTagsPopoverProps {
  /** The file's name: the popover is «Etiquetas de {name}». */
  name: string;
  tags: readonly string[];
  /** This game's other tags, then a rip's title and artist. */
  suggestions: readonly string[];
  open: boolean;
  onOpenChange(open: boolean): void;
  /** The edited tags, once, when the popover closes with a change. */
  onSave(tags: string[]): void;
  /** Puts the focus back where the popover was opened from (the row),
   * unless it closed because the user clicked or tabbed elsewhere. */
  onReturnFocus?: (() => void) | undefined;
  /** The element the popover hangs from (the row). */
  children: ReactElement;
}

/**
 * «Etiquetas…» of one extra (extras spec §8.3, D20): the tags as chips with
 * a remove button, an input where `Enter` adds one and `Backspace` on an
 * empty input removes the last, and the game's other tags below to add with
 * one click. The change is saved as one write when the popover closes.
 */
export function ExtraTagsPopover({
  name,
  tags,
  suggestions,
  open,
  onOpenChange,
  onSave,
  onReturnFocus,
  children,
}: ExtraTagsPopoverProps) {
  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverAnchor asChild>{children}</PopoverAnchor>
      {open && (
        <TagsEditor
          name={name}
          tags={tags}
          suggestions={suggestions}
          onReturnFocus={onReturnFocus}
          onClose={(draft) => {
            if (draft) onSave(draft);
            onOpenChange(false);
          }}
        />
      )}
    </Popover>
  );
}

function TagsEditor({
  name,
  tags,
  suggestions,
  onReturnFocus,
  onClose,
}: {
  name: string;
  tags: readonly string[];
  suggestions: readonly string[];
  onReturnFocus?: (() => void) | undefined;
  /** The edited tags when they changed, else null. */
  onClose(draft: string[] | null): void;
}) {
  const t = useT();
  const tr = useRichT();
  const id = useId();
  const [draft, setDraft] = useState<string[]>([...tags]);
  const [text, setText] = useState('');
  const outside = useRef(false);
  const full = draft.length >= MAX_TAGS;
  const have = new Set(draft.map(nameKey));
  const offered = suggestions
    .filter((s) => !have.has(nameKey(s)))
    .slice(0, SUGGEST);

  function add(raw: string) {
    const next = normalizeTags([...draft, raw]);
    if (!next.ok) return;
    setDraft(next.tags);
    setText('');
  }

  function close() {
    const changed =
      draft.length !== tags.length || draft.some((d, i) => d !== tags[i]);
    onClose(changed ? draft : null);
  }

  function onKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (text.trim() !== '') add(text);
    } else if (e.key === 'Backspace' && text === '' && draft.length > 0) {
      e.preventDefault();
      setDraft(draft.slice(0, -1));
    }
  }

  return (
    <PopoverContent
      align="start"
      className="extras-tagpop"
      aria-labelledby={`${id}-h`}
      onEscapeKeyDown={(e) => {
        // Closes the popover only, never the panel behind it.
        e.preventDefault();
        close();
      }}
      onInteractOutside={() => {
        outside.current = true;
        close();
      }}
      onCloseAutoFocus={(e) => {
        // The popover hangs from the row, not a trigger: Radix would drop
        // the focus on the page. Back to the row unless the user left.
        if (outside.current || !onReturnFocus) return;
        e.preventDefault();
        onReturnFocus();
      }}
      onKeyDown={(e) => {
        // The library's shortcuts never see what is typed here.
        e.stopPropagation();
      }}
    >
      <p id={`${id}-h`} className="extras-tagpop__h">
        {t('extras.tagsLabel', { name })}
      </p>
      {draft.length > 0 && (
        <ul className="extras-tags extras-tagpop__chips">
          {draft.map((tag) => (
            <li key={tag} className="extras-tag extras-tag--edit">
              {tag}
              <button
                type="button"
                className="extras-tag__x"
                aria-label={t('extras.tagRemove', { tag })}
                onClick={() =>
                  setDraft(draft.filter((d) => nameKey(d) !== nameKey(tag)))
                }
              >
                <X aria-hidden />
              </button>
            </li>
          ))}
        </ul>
      )}
      <Input
        size="sm"
        type="text"
        name="extras-tag"
        // eslint-disable-next-line jsx-a11y/no-autofocus -- the popover opened to type a tag
        autoFocus
        value={text}
        // Read-only, not disabled: a disabled input would drop the focus.
        readOnly={full}
        aria-describedby={full ? `${id}-full` : undefined}
        aria-label={t('extras.tagAdd')}
        placeholder={t('extras.tagAdd')}
        maxLength={64}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKeyDown}
      />
      {full && (
        <p id={`${id}-full`} className="panel-note extras-tagpop__note">
          {tr('extras.tagLimit', { n: MAX_TAGS })}
        </p>
      )}
      {offered.length > 0 && !full && (
        <div className="extras-tagpop__suggest">
          <p className="extras-tagpop__sub">{t('extras.tagSuggestions')}</p>
          <ul className="extras-tags">
            {offered.map((s) => (
              <li key={s}>
                <button
                  type="button"
                  className="extras-tag extras-tag--offer"
                  onClick={() => add(s)}
                >
                  {s}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </PopoverContent>
  );
}
