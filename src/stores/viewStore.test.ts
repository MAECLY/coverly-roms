import { beforeEach, describe, expect, it, vi } from 'vitest';

import { MemoryStorage } from '@/test/storage';

type Store = typeof import('./viewStore');
type Prefs = typeof import('@/lib/viewPrefs');

let storage: MemoryStorage;
let store: Store['useViewStore'];
let prefs: Prefs;

async function load(): Promise<void> {
  vi.resetModules();
  vi.stubGlobal('localStorage', storage);
  prefs = await import('@/lib/viewPrefs');
  store = (await import('./viewStore')).useViewStore;
}

describe('viewStore prefs', () => {
  beforeEach(async () => {
    vi.unstubAllGlobals();
    storage = new MemoryStorage();
    await load();
  });

  it('starts from the stored last prefs', async () => {
    storage.setItem('coverly.layout', 'list');
    await load();
    expect(store.getState().prefs.view).toBe('details');
    expect(store.getState().layout).toBe('list');
  });

  it('persists setView under the loaded root', () => {
    store.getState().loadFor('/a');
    store.getState().setView('shelves');
    const file = prefs.readPrefsFile(storage);
    expect(file.byLibrary['/a']?.view).toBe('shelves');
    expect(file.last.view).toBe('shelves');
    expect(store.getState().root).toBe('/a');
  });

  it('opens an unseen library with the last prefs and keeps libraries apart', () => {
    store.getState().loadFor('/a');
    store.getState().setView('columns');
    store.getState().loadFor('/b');
    expect(store.getState().prefs.view).toBe('columns');
    store.getState().setView('gallery');
    store.getState().loadFor('/a');
    expect(store.getState().prefs.view).toBe('columns');
    store.getState().loadFor('/b');
    expect(store.getState().prefs.view).toBe('gallery');
  });

  it('remembers prefs across a restart', async () => {
    store.getState().loadFor('/a');
    store.getState().setPrefs({ coverSize: 'xl', groupBy: 'system' });
    await load();
    store.getState().loadFor('/a');
    expect(store.getState().prefs).toMatchObject({
      coverSize: 'xl',
      groupBy: 'system',
    });
  });

  it('sanitises patches', () => {
    store.getState().setPrefs({
      view: 'bogus' as never,
      density: 'compact',
    });
    expect(store.getState().prefs.view).toBe('covers');
    expect(store.getState().prefs.density).toBe('compact');
  });

  it('saves only last before any library is loaded', () => {
    store.getState().setView('mounts');
    const file = prefs.readPrefsFile(storage);
    expect(file.last.view).toBe('mounts');
    expect(file.byLibrary).toEqual({});
  });

  it('reset() keeps prefs and root', () => {
    store.getState().loadFor('/a');
    store.getState().setView('gallery');
    store.getState().openDrawer();
    store.getState().reset();
    expect(store.getState().prefs.view).toBe('gallery');
    expect(store.getState().root).toBe('/a');
    expect(store.getState().drawerOpen).toBe(false);
  });

  it('keeps the layout alias in step with prefs.view', () => {
    expect(store.getState().layout).toBe('grid');
    store.getState().setView('details');
    expect(store.getState().layout).toBe('list');
    store.getState().setView('shelves');
    expect(store.getState().layout).toBe('list');
    store.getState().setLayout('grid');
    expect(store.getState().prefs.view).toBe('covers');
    store.getState().setLayout('list');
    expect(store.getState().prefs.view).toBe('details');
  });

  it('survives a storage that throws', async () => {
    const bad = {
      getItem: () => {
        throw new Error('x');
      },
      setItem: () => {
        throw new Error('x');
      },
    } as unknown as MemoryStorage;
    storage = bad;
    await load();
    expect(store.getState().prefs.view).toBe('covers');
    expect(() => store.getState().setView('details')).not.toThrow();
    expect(store.getState().prefs.view).toBe('details');
  });
});
