import { vi } from 'vitest';

type SizeProp = 'offsetWidth' | 'offsetHeight' | 'clientWidth' | 'clientHeight';

/** Gives every element a `width`×`height` layout box; returns the undo. */
export function stubLayout(width: number, height: number): () => void {
  const props: [SizeProp, number][] = [
    ['offsetWidth', width],
    ['offsetHeight', height],
    ['clientWidth', width],
    ['clientHeight', height],
  ];
  const saved = props.map(
    ([p]) =>
      [p, Object.getOwnPropertyDescriptor(HTMLElement.prototype, p)] as const,
  );
  for (const [p, value] of props) {
    Object.defineProperty(HTMLElement.prototype, p, {
      configurable: true,
      get: () => value,
    });
  }
  const rect = vi
    .spyOn(HTMLElement.prototype, 'getBoundingClientRect')
    .mockReturnValue({
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: width,
      bottom: height,
      width,
      height,
      toJSON: () => ({}),
    } as DOMRect);
  return () => {
    rect.mockRestore();
    for (const [p, descriptor] of saved) {
      if (descriptor)
        Object.defineProperty(HTMLElement.prototype, p, descriptor);
      else Reflect.deleteProperty(HTMLElement.prototype, p);
    }
  };
}
