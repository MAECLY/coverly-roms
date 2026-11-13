import { useEffect, type RefObject } from 'react';

import { useReducedMotion } from '@/lib/useReducedMotion';
import { useViewStore, type PanelSection } from '@/stores/viewStore';

function sectionIn(root: HTMLElement, section: PanelSection) {
  return root.querySelector<HTMLElement>(`[data-panel-section="${section}"]`);
}

/**
 * Scrolls the panel to the section a view asked for (a click on a cover's
 * add-on summary opens it at Versión, addon views §8.2) and moves the focus
 * to its heading. The section may render after the panel (its facts load
 * on their own), so the request waits for it; it is dropped once served,
 * or when the panel opens or closes another way.
 */
export function usePanelSection(root: RefObject<HTMLElement | null>): void {
  const section = useViewStore((s) => s.panelSection);
  const clear = useViewStore((s) => s.clearPanelSection);
  const reduced = useReducedMotion();
  useEffect(() => {
    const host = root.current;
    if (section === null || !host) return;
    const serve = (el: HTMLElement) => {
      el.scrollIntoView?.({
        block: 'start',
        behavior: reduced ? 'auto' : 'smooth',
      });
      const heading = el.querySelector<HTMLElement>('h2, h3, h4') ?? el;
      if (!heading.hasAttribute('tabindex')) heading.tabIndex = -1;
      heading.focus({ preventScroll: true });
      clear();
    };
    const now = sectionIn(host, section);
    if (now) {
      serve(now);
      return;
    }
    const watch = new MutationObserver(() => {
      const el = sectionIn(host, section);
      if (!el) return;
      watch.disconnect();
      serve(el);
    });
    watch.observe(host, { childList: true, subtree: true });
    return () => watch.disconnect();
  }, [section, root, clear, reduced]);
}
