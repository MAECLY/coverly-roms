import { afterEach, describe, expect, it, vi } from 'vitest';

import { applyTheme, resolveTheme } from './theme';

describe('theme', () => {
  afterEach(() => {
    document.documentElement.removeAttribute('data-theme');
    document.documentElement.classList.remove('dark');
    vi.unstubAllGlobals();
  });

  it('resolves system against the OS preference', () => {
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
    expect(resolveTheme('light', true)).toBe('light');
    expect(resolveTheme('dark', false)).toBe('dark');
  });

  it('marks the document and follows OS changes only for system', () => {
    const listeners: (() => void)[] = [];
    const mq = {
      matches: false,
      addEventListener: (_: string, cb: () => void) => listeners.push(cb),
      removeEventListener: vi.fn(),
    };
    vi.stubGlobal('matchMedia', () => mq);
    const stop = applyTheme('system');
    expect(document.documentElement.dataset.theme).toBe('light');
    mq.matches = true;
    listeners.forEach((cb) => cb());
    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    stop();
    expect(mq.removeEventListener).toHaveBeenCalled();
    applyTheme('light')();
    expect(document.documentElement.dataset.theme).toBe('light');
  });
});
