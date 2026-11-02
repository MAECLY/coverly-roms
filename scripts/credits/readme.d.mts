/** Types for readme.mjs, so TypeScript tests can import it. */
export const START: string;
export const END: string;
export const READMES: readonly { file: string; locale: 'es' | 'en' }[];
export const GROUPS: readonly string[];
export function renderBlock(root: string, locale: 'es' | 'en'): string;
export function replaceBlock(text: string, block: string): string;
export function checkReadmes(root: string): string[];
export function writeReadmes(root: string): void;
