import { act, render, screen } from '@testing-library/react';
import { useRef } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';

import { ExtrasDropLayer, type SectionDrop } from './ExtrasDropLayer';

let b: MockBackend;

function rect(el: Element, x: number, y: number, w: number, h: number) {
  (el as HTMLElement).getBoundingClientRect = () => new DOMRect(x, y, w, h);
}

function Harness(props: {
  enabled?: boolean;
  onOver(key: string | null): void;
  onDrop(d: SectionDrop): void;
  announce(text: string): void;
}) {
  const ref = useRef<HTMLElement>(null);
  return (
    <section ref={ref} data-testid="root">
      <div data-extras-band="fixed:soundtrack" data-testid="band" />
      <button
        type="button"
        data-extras-band="fixed:artwork"
        data-testid="slot"
      />
      <ExtrasDropLayer
        rootRef={ref}
        enabled={props.enabled ?? true}
        onOver={props.onOver}
        onDrop={props.onDrop}
        announce={props.announce}
      />
    </section>
  );
}

async function mount(enabled = true) {
  const onOver = vi.fn();
  const onDrop = vi.fn();
  const announce = vi.fn();
  render(
    <I18nProvider locale="es">
      <Harness
        enabled={enabled}
        onOver={onOver}
        onDrop={onDrop}
        announce={announce}
      />
    </I18nProvider>,
  );
  rect(screen.getByTestId('root'), 0, 0, 400, 400);
  rect(screen.getByTestId('band'), 0, 50, 400, 100);
  rect(screen.getByTestId('slot'), 0, 300, 120, 24);
  // The layer subscribes once the backend answers.
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
  return { onOver, onDrop, announce };
}

describe('ExtrasDropLayer (D11 a)', () => {
  beforeEach(() => {
    b = createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage: null,
      pauseAfter: null,
    });
    setBackend(b);
  });
  afterEach(() => {
    setBackend(null);
  });

  it('highlights the category under the drag and announces the hint once', async () => {
    const { onOver, announce } = await mount();
    act(() => b.emitDragOver({ x: 20, y: 80 }));
    expect(onOver).toHaveBeenLastCalledWith('fixed:soundtrack');
    expect(
      screen.getByText('Suelta en una categoría para copiar'),
    ).toBeVisible();
    act(() => b.emitDragOver({ x: 30, y: 310 }));
    expect(onOver).toHaveBeenLastCalledWith('fixed:artwork');
    act(() => b.emitDragOver({ x: 30, y: 200 }));
    expect(onOver).toHaveBeenLastCalledWith(null);
    expect(announce).toHaveBeenCalledTimes(1);
    expect(announce).toHaveBeenCalledWith(
      'Suelta en una categoría para copiar',
    );
    act(() => b.emitDragOver(null));
    expect(
      screen.queryByText('Suelta en una categoría para copiar'),
    ).not.toBeInTheDocument();
    // A new drag announces again.
    act(() => b.emitDragOver({ x: 20, y: 80 }));
    expect(announce).toHaveBeenCalledTimes(2);
  });

  it('a drag outside the section highlights nothing', async () => {
    const { onOver, announce } = await mount();
    act(() => b.emitDragOver({ x: 900, y: 80 }));
    expect(onOver).not.toHaveBeenCalledWith('fixed:soundtrack');
    expect(announce).not.toHaveBeenCalled();
  });

  it('hands a drop over with its token, names and the category it hit', async () => {
    const { onDrop, onOver } = await mount();
    act(() => b.emitDragOver({ x: 10, y: 60 }));
    let token = 0;
    act(() => {
      token = b.emitDrop({
        names: ['Boss.flac'],
        position: { x: 10, y: 60 },
      });
    });
    expect(onDrop).toHaveBeenCalledWith({
      token,
      items: [{ name: 'Boss.flac', size: expect.any(Number), is_dir: false }],
      key: 'fixed:soundtrack',
    });
    expect(onOver).toHaveBeenLastCalledWith(null);
    act(() => {
      b.emitDrop({ names: ['Map.png'], position: { x: 10, y: 250 } });
    });
    expect(onDrop).toHaveBeenLastCalledWith(
      expect.objectContaining({ key: null }),
    );
  });

  it('ignores drops outside the section, under a dialog, or when it cannot write', async () => {
    const { onDrop } = await mount();
    act(() => {
      b.emitDrop({ names: ['a.flac'], position: { x: 900, y: 60 } });
    });
    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    document.body.append(dialog);
    act(() => {
      b.emitDrop({ names: ['a.flac'], position: { x: 10, y: 60 } });
    });
    dialog.remove();
    expect(onDrop).not.toHaveBeenCalled();
  });

  it('a section that cannot write takes no drop', async () => {
    const { onDrop, announce } = await mount(false);
    act(() => b.emitDragOver({ x: 20, y: 80 }));
    act(() => {
      b.emitDrop({ names: ['a.flac'], position: { x: 10, y: 60 } });
    });
    expect(onDrop).not.toHaveBeenCalled();
    expect(announce).not.toHaveBeenCalled();
  });
});
