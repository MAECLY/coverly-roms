import { useVirtualizer } from '@tanstack/react-virtual';
import { ChevronDown, ChevronUp } from 'lucide-react';
import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactElement,
  type ReactNode,
} from 'react';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useDebouncedValue } from '@/hooks/useDebouncedValue';
import { useRichT } from '@/i18n/rich';
import { useT } from '@/i18n/useT';
import {
  findMatches,
  loadNotices,
  type NoticeBlock,
  type NoticeMatch,
  type Notices,
  type NoticesKind,
} from '@/lib/notices';

const KINDS: readonly NoticesKind[] = ['rust', 'js'];
const TAB_KEY: Record<NoticesKind, string> = {
  rust: 'notices.tabRust',
  js: 'notices.tabJs',
};
/** Rough line height of the caption mono text, for the first layout. */
const LINE_PX = 18;

/** `body[from, to)` with the matches inside it wrapped in `<mark>`. */
function marked(
  body: string,
  from: number,
  to: number,
  matches: readonly NoticeMatch[],
  current: NoticeMatch | undefined,
): ReactNode[] {
  const out: ReactNode[] = [];
  let at = from;
  for (const m of matches) {
    if (m.end <= from || m.start >= to) continue;
    const start = Math.max(m.start, from);
    const end = Math.min(m.end, to);
    if (start > at) out.push(body.slice(at, start));
    out.push(
      <mark
        key={start}
        className="notices__mark"
        data-current={m === current ? '' : undefined}
      >
        {body.slice(start, end)}
      </mark>,
    );
    at = end;
  }
  if (at < to) out.push(body.slice(at, to));
  return out;
}

/** [start, end) of each package line inside a block's `body`. */
function packageRanges(packages: readonly string[]): [number, number][] {
  const out: [number, number][] = [];
  let at = 0;
  for (const p of packages) {
    out.push([at, at + p.length]);
    at += p.length + 1;
  }
  return out;
}

function NoticeArticle({
  b,
  index,
  matches,
  current,
}: {
  b: NoticeBlock;
  index: number;
  matches: readonly NoticeMatch[];
  current: NoticeMatch | undefined;
}) {
  // Offsets into `body`: one line per package, the licence, a blank line,
  // then the text (see parseNotices).
  const packages = packageRanges(b.packages);
  const licenseFrom = packages.at(-1)?.[1] ?? 0;
  const licenseStart = licenseFrom + (packages.length > 0 ? 1 : 0);
  const textFrom = b.headLength + 2;
  return (
    <>
      <div className="notices__head">
        <p className="notices__packages">
          {packages.map(([from, to], i) => (
            <span key={from}>
              {i > 0 && ', '}
              {marked(b.body, from, to, matches, current)}
            </span>
          ))}
        </p>
        <p className="notices__license">
          {marked(b.body, licenseStart, b.headLength, matches, current)}
        </p>
      </div>
      <div className="notices__text" data-block-text={index}>
        {marked(b.body, textFrom, b.body.length, matches, current)}
      </div>
    </>
  );
}

function NoticesList({
  notices,
  label,
  matches,
  current,
}: {
  notices: Notices;
  /** The tab's name: the scrollable text is a named, focusable region. */
  label: string;
  matches: readonly NoticeMatch[];
  current: NoticeMatch | undefined;
}) {
  const scroller = useRef<HTMLDivElement | null>(null);
  const byBlock = useMemo(() => {
    const map = new Map<number, NoticeMatch[]>();
    for (const m of matches) {
      const list = map.get(m.block);
      if (list) list.push(m);
      else map.set(m.block, [m]);
    }
    return map;
  }, [matches]);
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual is not memoisable by the React Compiler
  const virtualizer = useVirtualizer({
    count: notices.blocks.length,
    getScrollElement: () => scroller.current,
    estimateSize: (i) => {
      const b = notices.blocks[i];
      return b ? (b.body.split('\n').length + 2) * LINE_PX : LINE_PX;
    },
    overscan: 4,
  });
  // Jump: bring the block of the current match in, then the match itself.
  useEffect(() => {
    if (!current) return;
    virtualizer.scrollToIndex(current.block, { align: 'start' });
    const frame = requestAnimationFrame(() => {
      const mark = scroller.current?.querySelector('mark[data-current]');
      if (mark && 'scrollIntoView' in mark) {
        mark.scrollIntoView({ block: 'center' });
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [current, virtualizer]);
  const items = virtualizer.getVirtualItems();
  return (
    // Focusable so the keyboard can scroll the text (no other control
    // inside it takes focus).
    <div
      ref={scroller}
      role="region"
      aria-label={label}
      // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- scrollable region, scrolled with the arrow keys
      tabIndex={0}
      className="notices__scroll"
    >
      <div
        className="relative w-full"
        style={{ height: virtualizer.getTotalSize() }}
      >
        {items.map((item) => {
          const b = notices.blocks[item.index];
          if (!b) return null;
          return (
            <article
              key={item.key}
              ref={virtualizer.measureElement}
              data-index={item.index}
              data-block={item.index}
              className="notices__block"
              style={{ transform: `translateY(${item.start}px)` }}
            >
              <NoticeArticle
                b={b}
                index={item.index}
                matches={byBlock.get(item.index) ?? []}
                current={current?.block === item.index ? current : undefined}
              />
            </article>
          );
        })}
      </div>
    </div>
  );
}

function NoticesBody() {
  const t = useT();
  const tr = useRichT();
  const findId = useId();
  const [kind, setKind] = useState<NoticesKind>('rust');
  const [files, setFiles] = useState<Partial<Record<NoticesKind, Notices>>>({});
  const [query, setQuery] = useState('');
  const settled = useDebouncedValue(query, 150);
  const [index, setIndex] = useState(0);

  useEffect(() => {
    let live = true;
    for (const k of KINDS) {
      void loadNotices(k).then((n) => {
        if (live) setFiles((prev) => ({ ...prev, [k]: n }));
      });
    }
    return () => {
      live = false;
    };
  }, []);

  const notices = files[kind];
  const matches = useMemo(
    () => (notices ? findMatches(notices.blocks, settled) : []),
    [notices, settled],
  );
  // A new search or another tab starts again at the first match.
  const [seen, setSeen] = useState({ matches, kind });
  if (seen.matches !== matches || seen.kind !== kind) {
    setSeen({ matches, kind });
    setIndex(0);
  }
  const current = matches[Math.min(index, matches.length - 1)];

  const step = (by: number) => {
    if (matches.length === 0) return;
    setIndex((i) => (i + by + matches.length) % matches.length);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    step(e.shiftKey ? -1 : 1);
  };

  return (
    <Tabs
      value={kind}
      onValueChange={(v) => setKind(v as NoticesKind)}
      className="min-h-0 flex-1"
    >
      <TabsList aria-label={t('notices.title')}>
        {KINDS.map((k) => (
          <TabsTrigger key={k} value={k}>
            {/* One inline run: the trigger is a flex row with a gap. */}
            <span>
              {tr(TAB_KEY[k], { count: files[k]?.packageCount ?? '—' })}
            </span>
          </TabsTrigger>
        ))}
      </TabsList>
      <div className="notices__find">
        <label htmlFor={findId} className="notices__find-label">
          {t('notices.find')}
        </label>
        <div className="notices__find-row">
          <Input
            id={findId}
            type="search"
            autoComplete="off"
            spellCheck={false}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            data-autofocus=""
          />
          <Button
            variant="ghost"
            size="icon"
            aria-label={t('notices.previous')}
            disabled={matches.length === 0}
            onClick={() => step(-1)}
          >
            <ChevronUp aria-hidden />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            aria-label={t('notices.next')}
            disabled={matches.length === 0}
            onClick={() => step(1)}
          >
            <ChevronDown aria-hidden />
          </Button>
          <p
            aria-live="polite"
            data-testid="notices-status"
            className="notices__status"
          >
            {settled.trim() === ''
              ? ''
              : matches.length === 0
                ? t('notices.noMatches')
                : tr('notices.matches', {
                    index: Math.min(index, matches.length - 1) + 1,
                    count: matches.length,
                  })}
          </p>
        </div>
      </div>
      {KINDS.map((k) => (
        <TabsContent
          key={k}
          value={k}
          className="min-h-0 flex-col data-[state=active]:flex"
          tabIndex={-1}
        >
          {notices && k === kind ? (
            <NoticesList
              notices={notices}
              label={t(TAB_KEY[k], { count: notices.packageCount })}
              matches={matches}
              current={current}
            />
          ) : (
            <p className="notices__scroll notices__loading">
              {t('notices.loading')}
            </p>
          )}
        </TabsContent>
      ))}
    </Tabs>
  );
}

/**
 * «Licencias de terceros» (spec §9.1 «Notices viewer»): the generated
 * Rust and JavaScript notices, one virtualised block per licence text, with
 * an in-memory find. `children` is the button that opens it; Radix returns
 * focus to it on close.
 */
export function NoticesDialog({ children }: { children: ReactElement }) {
  const t = useT();
  return (
    <Dialog>
      <DialogTrigger asChild>{children}</DialogTrigger>
      <DialogContent
        size="xl"
        className="notices"
        onOpenAutoFocus={(e) => {
          // Focus starts on the find field, not the first tab.
          const field = (
            e.currentTarget as HTMLElement | null
          )?.querySelector<HTMLElement>('[data-autofocus]');
          if (field) {
            e.preventDefault();
            field.focus();
          }
        }}
      >
        <DialogHeader>
          <DialogTitle>{t('notices.title')}</DialogTitle>
          <DialogDescription>{t('notices.description')}</DialogDescription>
        </DialogHeader>
        <NoticesBody />
      </DialogContent>
    </Dialog>
  );
}
