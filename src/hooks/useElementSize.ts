import { useCallback, useRef, useState, type RefObject } from 'react';

export interface Size {
  width: number;
  height: number;
}

/** A callback ref, the element's size (kept current) and the element. */
export function useElementSize<T extends HTMLElement>(): [
  (el: T | null) => void,
  Size,
  RefObject<T | null>,
] {
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });
  const element = useRef<T | null>(null);
  const observer = useRef<ResizeObserver | null>(null);
  const ref = useCallback((el: T | null) => {
    observer.current?.disconnect();
    observer.current = null;
    element.current = el;
    if (!el) return;
    const measure = () => {
      const next = {
        width: el.clientWidth || el.offsetWidth,
        height: el.clientHeight || el.offsetHeight,
      };
      setSize((prev) =>
        prev.width === next.width && prev.height === next.height ? prev : next,
      );
    };
    measure();
    if (typeof ResizeObserver !== 'undefined') {
      observer.current = new ResizeObserver(measure);
      observer.current.observe(el);
    }
  }, []);
  return [ref, size, element];
}
