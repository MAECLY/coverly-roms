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
