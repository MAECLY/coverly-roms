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
