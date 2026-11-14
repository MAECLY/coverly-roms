import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from 'react';

import { useT } from '@/i18n/useT';
import { getBackend } from '@/ipc/backend';
import { bandAt, modalOpen, pointIn } from '@/ipc/dropTarget';
import type { DroppedItem } from '@/ipc/types';

/** An OS drop that landed in the section, hit-tested to a category. */
export interface SectionDrop {
  token: number;
  items: DroppedItem[];
  /** The `data-extras-band` key of the category (band or dashed slot) it
   * landed on; null for anywhere else in the section. */
  key: string | null;
}

/**
 * The OS drag and drop of the Extras section (D11 a). Tauri's `over`
 * events, in CSS pixels (the backend divides by `devicePixelRatio`), drive
 * the highlight of the category under the pointer, since HTML5 `dragover`
 * never fires while Tauri handles drops; the hint is shown and announced
 * once per drag. A drop is hit-tested against the categories' boxes and
 * handed over with Rust's token: the section never sees, nor sends, a
 * path. Nothing happens while a dialog or the viewer is open, outside the
 * section, or when the section cannot write.
 */
export function ExtrasDropLayer({
  rootRef,
  enabled,
  onOver,
  onDrop,
  announce,
}: {
  rootRef: RefObject<HTMLElement | null>;
  enabled: boolean;
  onOver(key: string | null): void;
  onDrop(drop: SectionDrop): void;
  announce(text: string): void;
}) {
  const t = useT();
  const [inside, setInside] = useState(false);
  // The latest props, read by listeners subscribed once.
  const latest = useRef({ enabled, onOver, onDrop, announce, t });
  useLayoutEffect(() => {
    latest.current = { enabled, onOver, onDrop, announce, t };
  });

  useEffect(() => {
    let hinted = false;
    let over: string | null = null;
    const setOver = (key: string | null) => {
      if (key === over) return;
      over = key;
      latest.current.onOver(key);
    };
    const leave = () => {
      hinted = false;
      setOver(null);
      setInside(false);
    };
    let alive = true;
    const offs: (() => void)[] = [];
    void getBackend()
      .then(async (b) => {
        const offOver = await b.onExtrasDragOver((point) => {
          const root = rootRef.current;
          if (!point) {
            leave();
            return;
          }
          if (
            !root ||
            !latest.current.enabled ||
            modalOpen() ||
            !pointIn(root, point)
          ) {
            setOver(null);
            setInside(false);
            return;
          }
          setInside(true);
          setOver(bandAt(root, point));
          if (!hinted) {
            hinted = true;
            latest.current.announce(latest.current.t('extras.dropHint'));
          }
        });
        const offDrop = await b.onExtrasDrop((e) => {
          const root = rootRef.current;
          leave();
          if (!root || !latest.current.enabled || modalOpen()) return;
          if (!pointIn(root, e.position)) return;
          latest.current.onDrop({
            token: e.token,
            items: e.items,
            key: bandAt(root, e.position),
          });
        });
        if (alive) offs.push(offOver, offDrop);
        else {
          offOver();
          offDrop();
        }
      })
      // A backend that cannot report OS drags takes no drops here.
      .catch(() => undefined);
    return () => {
      alive = false;
      offs.forEach((off) => off());
    };
  }, [rootRef]);

  if (!inside) return null;
  return (
    <p className="extras-drophint" aria-hidden>
      {t('extras.dropHint')}
    </p>
  );
}
