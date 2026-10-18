/** Types for build_tokens.mjs, so TypeScript tests and configs can import it. */
export type Tokens = Record<string, unknown>;

export interface ThemeColour {
  name: string;
  value: string;
  rgb: string | null;
}

export interface ShadcnColour {
  name: string;
  value: string;
  role: string;
  tailwind: string;
}

export const REPO: string;
export const SOURCE: string;
export const OUTPUTS: { app: string };
export function readTokens(path?: string): Tokens;
export function resolveValue(tokens: Tokens, value: unknown): unknown;
export function themeColours(
  tokens: Tokens,
  mode: 'light' | 'dark',
): ThemeColour[];
export function shadcnColours(
  tokens: Tokens,
  mode: 'light' | 'dark',
): ShadcnColour[];
export function tailwindColors(
  tokens: Tokens,
): Record<string, string | Record<string, string>>;
export function contrast(a: string, b: string): number;
export function contrastFailures(tokens: Tokens): string[];
export function buildCss(tokens: Tokens, kind: 'app'): string;
export function buildAll(tokens: Tokens): Record<string, string>;
export function readOutputs(): Record<string, string | null>;
