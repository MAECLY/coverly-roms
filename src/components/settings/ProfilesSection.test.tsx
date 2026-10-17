import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Root } from '@/Root';
import { setBackend } from '@/ipc/backend';
import { BackendError } from '@/ipc/errors';
import { FIXTURE_NOW } from '@/ipc/mock/fixture';
import { createMockBackend, type MockBackend } from '@/ipc/mock/mockBackend';
import { resetLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { resetSettingsStore, useSettingsStore } from '@/stores/settingsStore';
import { stubLayout } from '@/test/layout';
import { MemoryStorage } from '@/test/storage';

let backend: MockBackend;
let storage: MemoryStorage;
let restore: () => void;

async function openProfiles() {
  render(<Root />);
  await screen.findByRole('button', { name: 'Settings' });
  act(() => useNavStore.getState().openSettings('profiles'));
  const list = await screen.findByRole('list', { name: 'Layout profiles' });
  // The list renders before the profiles load; wait for its rows.
  await within(list).findAllByRole('listitem');
  return list;
}

describe('profiles editor', () => {
  beforeEach(() => {
    storage = new MemoryStorage();
    backend = createMockBackend({ delayMs: 0, now: FIXTURE_NOW, storage });
    setBackend(backend);
    resetSettingsStore();
    resetLibraryStore();
    // A previous test leaves the settings screen open.
    useNavStore.getState().closeSettings();
    restore = stubLayout(1280, 800);
  });
  afterEach(() => {
    restore();
    vi.restoreAllMocks();
    setBackend(null);
  });

  it('lists the built-ins read-only, each duplicable', async () => {
    const list = await openProfiles();
    const onion = within(list).getByRole('listitem', { name: 'Onion OS' });
    expect(within(onion).getByText('Built-in (read-only)')).toBeInTheDocument();
    expect(
      within(onion).getByRole('button', { name: 'Duplicate Onion OS' }),
    ).toBeInTheDocument();
    expect(
      within(onion).queryByRole('button', { name: /Edit|Delete/ }),
    ).not.toBeInTheDocument();
  });

  it('names a duplicate in the UI language', async () => {
    await backend.updateSettings({ general: { language: 'es' } });
    render(<Root />);
    await screen.findByRole('button', { name: 'Configuración' });
    act(() => useNavStore.getState().openSettings('profiles'));
    const list = await screen.findByRole('list', {
      name: 'Perfiles de estructura',
    });
    await userEvent.click(
      await within(list).findByRole('button', { name: 'Duplicar Onion OS' }),
    );
    expect(screen.getByRole('textbox', { name: 'Nombre' })).toHaveValue(
      'Onion OS (copia)',
    );
  });

  it('duplicates, previews live, saves and keeps the profile after a reload', async () => {
    const list = await openProfiles();
    await userEvent.click(
      within(list).getByRole('button', { name: 'Duplicate Onion OS' }),
    );
    const form = screen.getByRole('form', { name: 'Edit layout profile' });
    expect(
      within(form).getByRole('textbox', { name: 'Profile id' }),
    ).toHaveValue('onion-copy');
    expect(
      await within(form).findByText('Roms/SFC/Imgs/Zelda (USA).png'),
    ).toBeInTheDocument();
    const template = within(form).getByRole('textbox', {
      name: 'Cover path template',
    });
    await userEvent.clear(template);
    await userEvent.type(template, '{{system_dir}/covers/{{stem}.png');
    expect(
      await within(form).findByText('Roms/SFC/covers/Zelda (USA).png'),
    ).toBeInTheDocument();
    const sub = within(form).getByRole('textbox', {
      name: 'Example subfolder',
    });
    await userEvent.type(sub, 'Hacks');
    expect(
      await within(form).findByText('Roms/SFC/covers/Zelda (USA).png'),
    ).toBeInTheDocument();
    await userEvent.click(
      within(form).getByRole('button', { name: 'Save profile' }),
    );
    const after = await screen.findByRole('list', { name: 'Layout profiles' });
    expect(
      within(after).getByRole('listitem', { name: 'Onion OS (copy)' }),
    ).toBeInTheDocument();
    const again = await createMockBackend({
      delayMs: 0,
      now: FIXTURE_NOW,
      storage,
    }).listProfiles();
    expect(
      again.find((p) => p.profile.id === 'onion-copy')?.profile.slots.cover
        ?.template,
    ).toBe('{system_dir}/covers/{stem}.png');
  });

  it('shows errors for a template that leaves the library and does not save it', async () => {
    await openProfiles();
    await userEvent.click(screen.getByRole('button', { name: 'New profile' }));
    const form = screen.getByRole('form', { name: 'Edit layout profile' });
    const template = within(form).getByRole('textbox', {
      name: 'Cover path template',
    });
    await userEvent.clear(template);
    await userEvent.type(template, '{{root}/../x/{{stem}.png');
    expect(
      await within(form).findByText('This value is not valid.'),
    ).toBeInTheDocument();
    expect(within(form).queryByText(/x\/Zelda/)).not.toBeInTheDocument();
    await userEvent.click(
      within(form).getByRole('button', { name: 'Save profile' }),
    );
    expect(
      screen.getByRole('form', { name: 'Edit layout profile' }),
    ).toBeInTheDocument();
    expect(
      (await backend.listProfiles()).some((p) => p.profile.id === 'my-layout'),
    ).toBe(false);
  });

  it('edits and deletes a custom profile after a confirmation', async () => {
    await openProfiles();
    await userEvent.click(screen.getByRole('button', { name: 'New profile' }));
    await userEvent.click(screen.getByRole('button', { name: 'Save profile' }));
    let item = within(
      await screen.findByRole('list', { name: 'Layout profiles' }),
    ).getByRole('listitem', { name: 'My layout' });
    await userEvent.click(
      within(item).getByRole('button', { name: 'Edit My layout' }),
    );
    const name = screen.getByRole('textbox', { name: 'Name' });
    await userEvent.clear(name);
    await userEvent.type(name, 'My SD');
    await userEvent.click(screen.getByRole('button', { name: 'Save profile' }));
    item = within(
      await screen.findByRole('list', { name: 'Layout profiles' }),
    ).getByRole('listitem', { name: 'My SD' });
    await useSettingsStore
      .getState()
      .update({ library: { default_profile: 'my-layout' } });
    await userEvent.click(
      within(item).getByRole('button', { name: 'Delete My SD' }),
    );
    await userEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', {
        name: 'Delete profile',
      }),
    );
    expect(
      within(
        await screen.findByRole('list', { name: 'Layout profiles' }),
      ).queryByRole('listitem', { name: 'My SD' }),
    ).not.toBeInTheDocument();
    expect(
      useSettingsStore.getState().settings!.library.default_profile,
    ).toBeNull();
  });

  it('renders hostile profile names as text', async () => {
    await backend.saveProfile(
      {
        id: 'evil',
        name: '<img src=x onerror=alert(1)>',
        roms_root: 'ROMS',
        art_dirs: [],
        gamelist: false,
        slots: {
          cover: {
            template: '{system_dir}/{stem}.png',
            format: 'png',
            max_width: null,
            max_height: null,
          },
        },
      },
      null,
    );
    await openProfiles();
    expect(
      screen.getByText('<img src=x onerror=alert(1)>'),
    ).toBeInTheDocument();
    expect(document.querySelectorAll('img[src="x"], [onerror]').length).toBe(0);
  });

  it('keeps typed separators and sizes as text until they are valid', async () => {
    await openProfiles();
    await userEvent.click(screen.getByRole('button', { name: 'New profile' }));
    const form = screen.getByRole('form', { name: 'Edit layout profile' });
    const dirs = within(form).getByRole('textbox', {
      name: 'Art folders (never scanned as ROMs)',
    });
    await userEvent.type(dirs, ', Art');
    expect(dirs).toHaveValue('Imgs, Art');
    await userEvent.click(
      within(form).getByRole('checkbox', { name: 'Cover slot' }),
    );
    await userEvent.click(
      within(form).getByRole('checkbox', { name: 'Cover slot' }),
    );
    const width = within(form).getByRole('textbox', {
      name: 'Cover maximum width',
    });
    await userEvent.type(width, '3x');
    expect(width).toHaveValue('3x');
    expect(within(form).getAllByText('This value is not valid.').length).toBe(
      1,
    );
    await userEvent.click(
      within(form).getByRole('button', { name: 'Save profile' }),
    );
    expect(
      screen.getByRole('form', { name: 'Edit layout profile' }),
    ).toBeInTheDocument();
    expect(
      (await backend.listProfiles()).some((p) => p.profile.id === 'my-layout'),
    ).toBe(false);
  });

  it('shows a failed save and keeps the form', async () => {
    vi.spyOn(backend, 'saveProfile').mockRejectedValue(
      new BackendError('io', 'disk full'),
    );
    await openProfiles();
    await userEvent.click(screen.getByRole('button', { name: 'New profile' }));
    await userEvent.click(screen.getByRole('button', { name: 'Save profile' }));
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(
      screen.getByRole('form', { name: 'Edit layout profile' }),
    ).toBeInTheDocument();
  });

  it('explains a mix of mirrored and flat slot templates', async () => {
    await openProfiles();
    await userEvent.click(screen.getByRole('button', { name: 'New profile' }));
    const form = screen.getByRole('form', { name: 'Edit layout profile' });
    await userEvent.click(
      within(form).getByRole('checkbox', { name: 'Screenshot slot' }),
    );
    const shot = within(form).getByRole('textbox', {
      name: 'Screenshot path template',
    });
    await userEvent.clear(shot);
    await userEvent.type(shot, '{{system_dir}/shots/{{stem}.png');
    expect(
      await within(form).findByText(
        'Either every slot template uses {rel_dir} or none does.',
      ),
    ).toBeInTheDocument();
    expect(
      within(form).queryByText(
        'This must not be higher than the auto-accept threshold.',
      ),
    ).not.toBeInTheDocument();
  });

  it('asks for at least one slot', async () => {
    await openProfiles();
    await userEvent.click(screen.getByRole('button', { name: 'New profile' }));
    const form = screen.getByRole('form', { name: 'Edit layout profile' });
    await userEvent.click(
      within(form).getByRole('checkbox', { name: 'Cover slot' }),
    );
    expect(
      await within(form).findByText('Turn on at least one slot.'),
    ).toBeInTheDocument();
  });

  it('sends one save while a save is in flight', async () => {
    let finish: () => void = () => undefined;
    const real = backend.saveProfile.bind(backend);
    const spy = vi
      .spyOn(backend, 'saveProfile')
      .mockImplementation(async (p, r) => {
        await new Promise<void>((resolve) => (finish = resolve));
        return real(p, r);
      });
    await openProfiles();
    await userEvent.click(screen.getByRole('button', { name: 'New profile' }));
    const save = screen.getByRole('button', { name: 'Save profile' });
    await userEvent.click(save);
    await userEvent.click(save);
    expect(save).toBeDisabled();
    await act(async () => finish());
    await screen.findByRole('list', { name: 'Layout profiles' });
    expect(spy).toHaveBeenCalledTimes(1);
  });
});
