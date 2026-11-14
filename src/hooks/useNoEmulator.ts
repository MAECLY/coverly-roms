import { setSystemNotes, useSystemNote } from './useSystemNote';

/** Whether no emulator opens this system; false for none or unknown. */
export const useNoEmulator = (systemId: string | null | undefined): boolean =>
  useSystemNote(systemId) === 'noEmulator';

/** Tests only: start from a known catalogue (`null`: load it again). */
export function setNoEmulatorIds(next: readonly string[] | null): void {
  setSystemNotes(next === null ? null : { noEmulator: next, pcOnly: [] });
}
