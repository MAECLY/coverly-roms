import { fireEvent, render } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { DEFAULT_PREFS, type ViewId, type ViewPrefs } from '@/lib/viewPrefs';

import { useViewShortcuts } from './useViewShortcuts';

function Harness({
  view = 'covers',
  coverSize = 'm',
  inspector = false,
  enabled,
  spy,
}: {
  view?: ViewId;
  coverSize?: ViewPrefs['coverSize'];
  inspector?: boolean;
  enabled?: boolean;
  spy: (patch: Partial<ViewPrefs>) => void;
}) {
  const [prefs, setState] = useState<ViewPrefs>({
    ...DEFAULT_PREFS,
    view,
    coverSize,
    inspector,
  });
  useViewShortcuts({
    prefs,
    setPrefs: (patch) => {
      spy(patch);
      setState((p) => ({ ...p, ...patch }));
    },
    ...(enabled === undefined ? {} : { enabled }),
  });
  return (
    <>
      <div data-testid="size">{prefs.coverSize}</div>
      <div data-testid="inspector">{String(prefs.inspector)}</div>
      <div data-testid="current">{prefs.view}</div>
      <input type="search" aria-label="Search" />
      <button type="button" data-testid="view">
        view
      </button>
    </>
  );
}

/** A keydown on `target` that reports whether it was preventDefault'ed. */
function press(
  target: Element | Window,
  key: string,
  init: KeyboardEventInit = { metaKey: true },
) {
  return !fireEvent.keyDown(target, { key, ...init });
}

function wheel(target: Element | Window, deltaY: number, ctrlKey = true) {
  return !fireEvent.wheel(target, { deltaY, ctrlKey });
}

describe('useViewShortcuts', () => {
  it('⌘+ and ⌘= grow the covers one stop, ⌘− shrinks, ⌘0 resets to M', () => {
    const spy = vi.fn();
    const { getByTestId } = render(<Harness spy={spy} />);
    const view = getByTestId('view');
    expect(press(view, '+')).toBe(true);
    expect(spy).toHaveBeenLastCalledWith({ coverSize: 'l' });
    expect(press(view, '=')).toBe(true);
    expect(spy).toHaveBeenLastCalledWith({ coverSize: 'xl' });
    expect(press(view, '-')).toBe(true);
    expect(spy).toHaveBeenLastCalledWith({ coverSize: 'l' });
    expect(press(view, '0')).toBe(true);
    expect(spy).toHaveBeenLastCalledWith({ coverSize: 'm' });
    expect(getByTestId('size')).toHaveTextContent('m');
  });

  it('accepts Ctrl as the modifier', () => {
    const spy = vi.fn();
    const { getByTestId } = render(<Harness spy={spy} />);
    expect(press(getByTestId('view'), '-', { ctrlKey: true })).toBe(true);
    expect(spy).toHaveBeenLastCalledWith({ coverSize: 's' });
  });

  it('works in Shelves too', () => {
    const spy = vi.fn();
    render(<Harness spy={spy} view="shelves" />);
    expect(press(window, '+')).toBe(true);
    expect(spy).toHaveBeenLastCalledWith({ coverSize: 'l' });
  });

  it('stays at XL and XS without asking for a change', () => {
    const spy = vi.fn();
    render(<Harness spy={spy} coverSize="xl" />);
    expect(press(window, '+')).toBe(true);
    expect(spy).not.toHaveBeenCalled();
  });

  it('does nothing in Details and leaves the key alone', () => {
    const spy = vi.fn();
    render(<Harness spy={spy} view="details" />);
    for (const key of ['+', '=', '-', '0']) {
      expect(press(window, key)).toBe(false);
    }
    expect(wheel(window, -300)).toBe(false);
    expect(spy).not.toHaveBeenCalled();
  });

  it('is ignored while focus is in the search box', () => {
    const spy = vi.fn();
    const { getByRole } = render(<Harness spy={spy} />);
    const search = getByRole('searchbox');
    search.focus();
    expect(press(search, '+')).toBe(false);
    expect(press(search, '0')).toBe(false);
    expect(spy).not.toHaveBeenCalled();
  });

  it('ignores the keys without a modifier, or with Alt', () => {
    const spy = vi.fn();
    render(<Harness spy={spy} />);
    expect(press(window, '+', {})).toBe(false);
    expect(press(window, '+', { metaKey: true, altKey: true })).toBe(false);
    expect(spy).not.toHaveBeenCalled();
  });

  it('stands down when disabled or behind an open dialog', () => {
    const spy = vi.fn();
    const { rerender } = render(<Harness spy={spy} enabled={false} />);
    expect(press(window, '+')).toBe(false);
    expect(wheel(window, -300)).toBe(false);
    rerender(<Harness spy={spy} />);
    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    dialog.setAttribute('data-state', 'open');
    document.body.append(dialog);
    expect(press(window, '+')).toBe(false);
    dialog.remove();
    expect(spy).not.toHaveBeenCalled();
  });

  it('ctrl+wheel moves one stop per 100px of accumulated delta', () => {
    const spy = vi.fn();
    const { getByTestId } = render(<Harness spy={spy} />);
    const view = getByTestId('view');
    // 250px towards the user (pinch in): two stops smaller, 50px kept.
    expect(wheel(view, 120)).toBe(true);
    expect(wheel(view, 80)).toBe(true);
    expect(wheel(view, 50)).toBe(true);
    expect(spy).toHaveBeenCalledTimes(2);
    expect(getByTestId('size')).toHaveTextContent('xs');
    // 50 more completes the third 100px, but XS is the end of the slider.
    expect(wheel(view, 50)).toBe(true);
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it('a wheel turn the other way starts counting again', () => {
    const spy = vi.fn();
    const { getByTestId } = render(<Harness spy={spy} />);
    wheel(window, 90);
    wheel(window, -60);
    expect(spy).not.toHaveBeenCalled();
    wheel(window, -40);
    expect(spy).toHaveBeenLastCalledWith({ coverSize: 'l' });
    expect(getByTestId('size')).toHaveTextContent('l');
  });

  it('a pause ends the gesture and drops the leftover delta', () => {
    vi.useFakeTimers({ toFake: ['Date', 'performance'] });
    try {
      const spy = vi.fn();
      render(<Harness spy={spy} />);
      wheel(window, 90);
      vi.advanceTimersByTime(1000);
      wheel(window, 90);
      expect(spy).not.toHaveBeenCalled();
      wheel(window, 10);
      expect(spy).toHaveBeenLastCalledWith({ coverSize: 's' });
    } finally {
      vi.useRealTimers();
    }
  });

  it('a plain wheel scrolls as usual', () => {
    const spy = vi.fn();
    render(<Harness spy={spy} />);
    expect(wheel(window, 300, false)).toBe(false);
    expect(spy).not.toHaveBeenCalled();
  });

  describe('⌘I', () => {
    it.each(['covers', 'details', 'mounts', 'shelves'] as const)(
      'toggles the docked inspector in %s',
      (view) => {
        const spy = vi.fn();
        const { getByTestId } = render(<Harness spy={spy} view={view} />);
        expect(press(getByTestId('view'), 'i')).toBe(true);
        expect(spy).toHaveBeenLastCalledWith({ inspector: true });
        expect(getByTestId('inspector')).toHaveTextContent('true');
        expect(press(window, 'I', { ctrlKey: true })).toBe(true);
        expect(spy).toHaveBeenLastCalledWith({ inspector: false });
      },
    );

    it.each(['columns', 'gallery'] as const)(
      'does nothing in %s, which have their own preview pane',
      (view) => {
        const spy = vi.fn();
        render(<Harness spy={spy} view={view} />);
        expect(press(window, 'i')).toBe(false);
        expect(spy).not.toHaveBeenCalled();
      },
    );

    it('is ignored while typing, without a modifier, with Shift and behind a dialog', () => {
      const spy = vi.fn();
      const { getByRole } = render(<Harness spy={spy} />);
      const search = getByRole('searchbox');
      search.focus();
      expect(press(search, 'i')).toBe(false);
      expect(press(window, 'i', {})).toBe(false);
      expect(press(window, 'I', { metaKey: true, shiftKey: true })).toBe(false);
      expect(press(window, 'I', { ctrlKey: true, shiftKey: true })).toBe(false);
      const dialog = document.createElement('div');
      dialog.setAttribute('role', 'dialog');
      dialog.setAttribute('data-state', 'open');
      document.body.append(dialog);
      expect(press(window, 'i')).toBe(false);
      dialog.remove();
      expect(spy).not.toHaveBeenCalled();
    });
  });
  describe('⌘1–⌘6', () => {
    function dialogOpen(role = 'dialog') {
      const el = document.createElement('div');
      el.setAttribute('role', role);
      el.setAttribute('data-state', 'open');
      document.body.append(el);
      return el;
    }

    it('Ctrl+3 and ⌘3 switch to Columns', () => {
      const spy = vi.fn();
      const { getByTestId } = render(<Harness spy={spy} />);
      expect(press(getByTestId('view'), '3', { ctrlKey: true })).toBe(true);
      expect(spy).toHaveBeenLastCalledWith({ view: 'columns' });
      expect(getByTestId('current')).toHaveTextContent('columns');
      expect(press(window, '1')).toBe(true);
      expect(press(window, '3')).toBe(true);
      expect(spy).toHaveBeenLastCalledWith({ view: 'columns' });
    });

    it('maps 1–6 to the views in switcher order', () => {
      const spy = vi.fn();
      render(<Harness spy={spy} view="shelves" />);
      const order = [
        'covers',
        'details',
        'columns',
        'gallery',
        'mounts',
        'shelves',
      ];
      order.forEach((view, i) => {
        expect(press(window, String(i + 1))).toBe(true);
        expect(spy).toHaveBeenLastCalledWith({ view });
      });
    });

    it('follows the physical key on layouts where the digit needs Shift', () => {
      const spy = vi.fn();
      render(<Harness spy={spy} />);
      // AZERTY: the 2 key without Shift types "é".
      expect(press(window, 'é', { metaKey: true, code: 'Digit2' })).toBe(true);
      expect(spy).toHaveBeenLastCalledWith({ view: 'details' });
    });

    it('works with focus in the search box, without typing a digit', () => {
      const spy = vi.fn();
      const { getByRole } = render(<Harness spy={spy} />);
      const search = getByRole('searchbox');
      search.focus();
      expect(press(search, '2', { ctrlKey: true })).toBe(true);
      expect(spy).toHaveBeenLastCalledWith({ view: 'details' });
    });

    it('takes the key but asks for nothing when the view is already shown', () => {
      const spy = vi.fn();
      render(<Harness spy={spy} />);
      expect(press(window, '1')).toBe(true);
      expect(spy).not.toHaveBeenCalled();
    });

    it('ignores 0, 7–9, Alt, Shift and a plain digit', () => {
      const spy = vi.fn();
      render(<Harness spy={spy} view="details" />);
      expect(press(window, '7')).toBe(false);
      expect(press(window, '3', {})).toBe(false);
      expect(press(window, '3', { metaKey: true, altKey: true })).toBe(false);
      expect(
        press(window, '#', { metaKey: true, shiftKey: true, code: 'Digit3' }),
      ).toBe(false);
      expect(spy).not.toHaveBeenCalled();
    });

    it.each(['dialog', 'alertdialog'])(
      'stands down behind an open Radix %s',
      (role) => {
        const spy = vi.fn();
        render(<Harness spy={spy} />);
        const el = dialogOpen(role);
        expect(press(window, '3', { ctrlKey: true })).toBe(false);
        el.remove();
        expect(spy).not.toHaveBeenCalled();
      },
    );

    it('stands down while disabled (an overlay is open)', () => {
      const spy = vi.fn();
      render(<Harness spy={spy} enabled={false} />);
      expect(press(window, '3', { ctrlKey: true })).toBe(false);
      expect(spy).not.toHaveBeenCalled();
    });

    it('leaves a key another handler already took', () => {
      const spy = vi.fn();
      const { getByTestId } = render(<Harness spy={spy} />);
      const view = getByTestId('view');
      const taken = (ev: Event) => ev.preventDefault();
      view.addEventListener('keydown', taken);
      press(view, '3', { ctrlKey: true });
      view.removeEventListener('keydown', taken);
      expect(spy).not.toHaveBeenCalled();
    });

    it('⌘+ in Details is still left alone', () => {
      const spy = vi.fn();
      render(<Harness spy={spy} view="details" />);
      expect(press(window, '+')).toBe(false);
      expect(spy).not.toHaveBeenCalled();
    });
  });
});
