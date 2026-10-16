import { invoke } from '@tauri-apps/api/core';

/** Mirror of the Rust `AppInfo` returned by the `app_info` command. */
export interface AppInfo {
  name: string;
  version: string;
  core_version: string;
}

/**
 * True inside the Tauri webview (or under `mockIPC` in tests). A plain browser
 * (`pnpm dev`) and jsdom have no IPC bridge, so calling `invoke` would throw.
 */
export function hasTauriIpc(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** Fetches the app and core versions, or `null` when there is no backend. */
export async function fetchAppInfo(): Promise<AppInfo | null> {
  if (!hasTauriIpc()) return null;
  try {
    return await invoke<AppInfo>('app_info');
  } catch {
    return null;
  }
}
