import '@testing-library/jest-dom/vitest';
import { beforeEach } from 'vitest';

// The mock backend persists settings in `localStorage`: every test starts
// from an empty one.
beforeEach(() => {
  localStorage.clear();
});

// jsdom lacks these browser APIs used by the explorer.
class ResizeObserverStub {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
if (typeof globalThis.ResizeObserver === 'undefined') {
  (globalThis as { ResizeObserver?: unknown }).ResizeObserver =
    ResizeObserverStub;
}
if (typeof URL.createObjectURL !== 'function') {
  URL.createObjectURL = () => 'blob:stub';
}
if (typeof URL.revokeObjectURL !== 'function') {
  URL.revokeObjectURL = () => undefined;
}
if (typeof Element.prototype.scrollTo !== 'function') {
  Element.prototype.scrollTo = () => undefined;
}

// Radix primitives (src/components/ui) use pointer capture and
// scrollIntoView; Sonner and the theme read matchMedia.
const elementProto = Element.prototype as unknown as Record<string, unknown>;
elementProto.hasPointerCapture ??= () => false;
elementProto.setPointerCapture ??= () => undefined;
elementProto.releasePointerCapture ??= () => undefined;
elementProto.scrollIntoView ??= () => undefined;
if (typeof window.matchMedia !== 'function') {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => undefined,
      removeListener: () => undefined,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      dispatchEvent: () => false,
    }) as MediaQueryList;
}
