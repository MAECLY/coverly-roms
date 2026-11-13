import { act, render, screen } from '@testing-library/react';
import { useRef, useState } from 'react';
import { afterEach, describe, expect, it } from 'vitest';

import { useViewStore } from '@/stores/viewStore';

import { usePanelSection } from './usePanelSection';

function Panel({ late = false }: { late?: boolean }) {
  const ref = useRef<HTMLElement>(null);
  const [shown, setShown] = useState(!late);
  usePanelSection(ref);
  return (
    <aside ref={ref}>
      <button type="button" onClick={() => setShown(true)}>
        load
      </button>
      {shown && (
        <section data-panel-section="version">
          <h3>Versión</h3>
        </section>
      )}
    </aside>
  );
}

describe('usePanelSection', () => {
  afterEach(() => {
    act(() => useViewStore.getState().closeDrawer());
  });

  it('focuses the asked section heading and drops the request', () => {
    render(<Panel />);
    act(() => useViewStore.getState().openDrawer('version'));
    expect(document.activeElement).toBe(screen.getByText('Versión'));
    expect(useViewStore.getState().panelSection).toBeNull();
  });

  it('waits for a section that renders later', async () => {
    render(<Panel late />);
    act(() => useViewStore.getState().openDrawer('version'));
    expect(useViewStore.getState().panelSection).toBe('version');
    await act(async () => screen.getByText('load').click());
    expect(document.activeElement).toBe(screen.getByText('Versión'));
    expect(useViewStore.getState().panelSection).toBeNull();
  });

  it('does nothing when the panel opens plainly', () => {
    render(<Panel />);
    act(() => useViewStore.getState().openDrawer());
    expect(document.activeElement).toBe(document.body);
  });
});
